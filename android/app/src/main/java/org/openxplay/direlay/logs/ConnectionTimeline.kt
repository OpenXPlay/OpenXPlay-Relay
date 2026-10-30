// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-23
package org.openxplay.direlay.logs

/** Pure attempt accounting. The owner serializes calls; timestamps are captured at the event source. */
class ConnectionTimeline(private val clock: () -> Long, private val processStart: Long = clock(),
    private val context: () -> String = { ConnectionContext().line() }) {
    private class Attempt(val number: Long, val start: Long) {
        var previous = start
        var terminal = false
        var summarized = false
        var active: Long? = null
        var reason = "closed"
        var lastStatus: String? = null
        var reached = "starting"
        var phoneLink = "wireless_run"
        var root: LinkEnd? = null
        var fallbackRoot: LinkEnd? = null
        var interrupted = false
        var pendingEnd: Long? = null
        val endedLinks = mutableSetOf<String>()
        val linkStarts = mutableMapOf<String, Long>()
        val marks = linkedMapOf<String, Long>()
    }
    private val attempts = mutableMapOf<String, Attempt>()
    private val pendingPhoneEnds = linkedMapOf<Long, Attempt>()
    private val transitions = linkedMapOf<String, String>()
    private val carlinkAttempts = linkedMapOf<Long, Attempt>()
    private val castAttempts = linkedMapOf<Long, Attempt>()
    private var nextCarlink = 0L
    private var nextCast = 0L
    private var latestConnection = 0L
    private class Ends(val start: Long, var end: LinkEnd, var attempt: Attempt) { var count = 0; var repeated = 0 }
    private val ends = linkedMapOf<String, Ends>()

    fun linkEnded(domain: String, end: LinkEnd, at: Long = clock(), connection: Long = 0): List<String> {
        val a = if (domain == "cast") castAttempt(connection, at) else attempts.getOrPut(domain) { Attempt(1, at) }
        if (domain == "phone") {
            reach(a, end.reached)
            if (end.cause != "teardown" && a.root == null) a.root = end
        }
        val result = emitEnd(domain, a, end, at).toMutableList()
        if (domain == "phone" && end.link in setOf("wireless_run", "usb") && (end.cause != "teardown" || "next" in end.facts)) {
            result += interrupt(a, end, end.facts["next"] ?: "none", end.facts["nextInMs"] ?: "-", at)
        }
        return result
    }

    /** A Relay decision is the fallback root, never a second end report for an upstream-owned link. */
    fun phoneDecision(end: LinkEnd, next: String, at: Long = clock()): List<String> {
        val a = attempts["phone"] ?: return emptyList()
        return interrupt(a, end, next, "-", at)
    }

    fun flushEnds(at: Long = clock(), force: Boolean = false): List<String> = buildList {
        val iterator = ends.iterator()
        while (iterator.hasNext()) {
            val (key, window) = iterator.next()
            if (!force && elapsed(window.start, at) < 60_000) continue
            if (window.repeated > 0) {
                val domain = key.substringBefore('/')
                add(line(domain, "link_ended", window.attempt, at, "${window.end.line()} repeated=${window.repeated}"))
            }
            iterator.remove()
        }
    }

    /** Let the run's next-step decision precede a failure/session-only fallback. */
    fun flush(at: Long = clock(), force: Boolean = false): List<String> = buildList {
        addAll(flushEnds(at, force))
        for (a in pendingPhoneEnds.values.toList()) {
            val ended = a.pendingEnd
            if (ended != null && (force || elapsed(ended, at) >= 2_000)) {
                addAll(interrupt(a, LinkEnd(a.phoneLink, a.reached, "local", "teardown"), "none", "-", at))
            }
        }
    }

    private fun pending(a: Attempt, at: Long, reset: Boolean = false) {
        if (a.interrupted) return
        if (reset || a.pendingEnd == null) a.pendingEnd = at
        pendingPhoneEnds[a.number] = a
    }

    private fun emitEnd(domain: String, a: Attempt, end: LinkEnd, at: Long): List<String> {
        val result = flushEnds(at).toMutableList()
        val key = "$domain/${end.link}"
        val window = ends.getOrPut(key) { Ends(at, end, a) }
        window.end = end; window.attempt = a
        if (++window.count <= 20) result += line(domain, "link_ended", a, at, end.line()) else window.repeated++
        return result
    }

    private fun interrupt(a: Attempt, fallback: LinkEnd, next: String, nextIn: String, at: Long): List<String> {
        if (a.interrupted) return emptyList()
        a.interrupted = true; a.terminal = true
        pendingPhoneEnds.remove(a.number)
        val root = a.root ?: a.fallbackRoot ?: fallback
        val action = when (next) { "rebuild", "wait_iphone", "retry", "stop", "none" -> next; "fail" -> "stop"; else -> "none" }
        val delay = nextIn.toLongOrNull()?.takeIf { it >= 0 }?.toString() ?: "-"
        return listOf(line("phone", "interrupted", a, at,
            "reached=${a.reached} link=${root.link} by=${root.by} cause=${root.cause} next=$action nextInMs=$delay${quotedDetail(root.detail)}"),
            "phone context attempt=${a.number} " + runCatching { context() }.getOrElse { ConnectionContext().line() })
    }

    private fun reach(a: Attempt, mark: String) {
        if ((phoneMarks[mark] ?: 0) > (phoneMarks[a.reached] ?: 0)) a.reached = mark
    }

    fun event(domain: String, event: String, detail: String = "", at: Long = clock(),
        startsAttempt: Boolean = false): List<String> {
        var a = attempts[domain]
        if (a == null || startsAttempt) {
            a = Attempt((a?.number ?: 0) + 1, if (domain == "relay" || domain == "network") processStart else at)
            attempts[domain] = a
        }
        val facts = if (domain == "relay" && event == "app_ready") "duration=${elapsed(a.start, at)} $detail".trimEnd() else detail
        return listOf(line(domain, event, a, at, facts))
    }

    fun transition(domain: String, key: String, event: String, detail: String = "", at: Long = clock()): List<String> {
        val value = "$event $detail"
        val id = "$domain/$key"
        val previous = transitions.remove(id)
        if (transitions.size >= 256) transitions.remove(transitions.keys.first())
        transitions[id] = value
        if (previous == value) return emptyList()
        return event(domain, event, detail, at)
    }

    fun phoneStatus(status: String, detail: String = "", at: Long = clock()): List<String> {
        val old = attempts["phone"]
        val begin = old == null || old.terminal && status !in setOf("Failed", "ControlEnded") ||
            status == "DiscoveringMfi" && "DiscoveringMfi" in old.marks && old.lastStatus != "$status $detail"
        if (begin) attempts["phone"] = Attempt((old?.number ?: 0) + 1, at)
        val a = attempts.getValue("phone")
        if (a.lastStatus == "$status $detail") return emptyList()
        a.lastStatus = "$status $detail"
        a.marks.putIfAbsent(status, at)
        reach(a, status.replace(Regex("([a-z])([A-Z])"), "$1_$2").lowercase())
        if (status == "Failed") { a.reason = detail; a.terminal = true }
        val failure = if ("cause=\"" in detail) detail else "cause=\"${connectionDetail(detail.substringAfter("reason=", detail))}\""
        val result = mutableListOf(line("phone", status, a, at, if (status == "Failed") failure else detail))
        if (status == "Failed") {
            result += line("phone", "failed", a, at, "reached=${a.reached} $failure")
            if (a.root == null && a.fallbackRoot == null) a.fallbackRoot = LinkEnd(a.phoneLink, a.reached, "local", "connect_failed",
                failure.substringAfter("cause=\"", "").substringBefore('"'))
            pending(a, at, reset = true)
        }
        return result
    }

    fun phone(event: String, detail: String = "", at: Long = clock()): List<String> {
        if (event in setOf("attempt_started", "session_active", "first_video") && attempts["phone"]?.terminal == true) {
            val old = attempts.getValue("phone")
            attempts["phone"] = Attempt(old.number + 1, at)
        }
        val a = attempts.getOrPut("phone") { Attempt(1, at) }
        if (event == "attempt_started") a.phoneLink = if ("link=usb" in detail) "usb" else "wireless_run"
        if (event in setOf("first_video", "session_active", "session_ended") && a.marks.containsKey(event)) return emptyList()
        a.marks.putIfAbsent(event, at)
        if (event == "startup_milestone") reach(a, detail.substringAfter("stage=", "").substringBefore(' ')) else reach(a, event)
        if (event == "session_active") a.active = at
        if (event == "session_ended") { a.terminal = true; if (a.active != null) pending(a, at) }
        val extra = if (event == "session_ended") "duration=${elapsed(a.active ?: a.start, at)}" else detail
        val result = mutableListOf(line("phone", event, a, at, extra))
        if (event in setOf("first_video", "session_active") && !a.summarized) {
            a.summarized = true
            val phases = listOf("mfi" to ("DiscoveringMfi" to "MfiReady"),
                "hotspot" to ("StartingHotspot" to "HotspotReady"),
                "bluetooth" to ("ConnectingBluetooth" to "RunningWireless"),
                "wireless" to ("RunningWireless" to (if ("WirelessActiveFallback" in a.marks) "WirelessActiveFallback" else "WirelessActive")))
            result += line("phone", "connected", a, at, summary(a, at, phases))
        }
        return result
    }

    fun carlink(message: String, at: Long = clock(), connection: Long = 0): List<String> {
        val event = message.substringBefore(' ')
        val detail = message.substringAfter(' ', "")
        val old = carlinkAttempts[connection]
        if (old == null && connection < latestConnection) return emptyList()
        latestConnection = maxOf(latestConnection, connection)
        if (old == null || event == "link_start") {
            if (old == null && carlinkAttempts.size >= 8) carlinkAttempts.remove(carlinkAttempts.keys.first())
            // Attempt zero describes output preparation before the first link_start.
            carlinkAttempts[connection] = Attempt(if (event == "link_start") ++nextCarlink else 0, at)
        }
        val a = carlinkAttempts.getValue(connection)
        if (event.startsWith("state=")) {
            if (a.lastStatus == event) return emptyList()
            a.lastStatus = event
        }
        a.marks.putIfAbsent(event, at)
        if ((carlinkMarks[event] ?: -1) > (carlinkMarks[a.reached] ?: -1)) a.reached = event
        val opening = when (event) { "link_start" -> "session"; "ble_scan_started" -> "ble"; "gatt_connect_started" -> "gatt"; "wifi_join" -> "wifi"; else -> null }
        if (opening != null) a.linkStarts[opening] = at
        when (event) { "ble_scan_started" -> a.endedLinks.remove("ble"); "gatt_connect_started" -> a.endedLinks.remove("gatt"); "wifi_join" -> a.endedLinks.remove("wifi") }
        if (message.contains(" failed ")) a.reason = detail.substringAfter("reason=", detail).substringBefore(' ')
        if (event == "link_lost") a.reason = event
        val end = carlinkEnd(message, connectionToken(a.reached.lowercase().substringAfter("state=")))?.let {
            if (it.age >= 0) it else it.copy(age = a.linkStarts[it.link]?.let { start -> elapsed(start, at) } ?: -1)
        }?.takeIf {
            "link=" in message || event !in setOf("gatt_closed", "gatt_disconnect", "network_request_released", "link_stop", "gatt_connection", "ble_scan", "ble_exchange", "wifi_join", "link_lost") || a.endedLinks.add(it.link)
        }
        if (end != null && end.cause != "teardown" && a.root == null) a.root = end
        val result = if (event == "link_ended" && end != null) emitEnd("carlink", a, end, at).toMutableList()
            else mutableListOf(line("carlink", event, a, at, detail))
        if (end != null && event !in setOf("link_ended", "decision", "session", "tcp_connect", "auth")) result += emitEnd("carlink", a, end, at)
        if (event == "state=PLAYING" && !a.summarized) {
            a.summarized = true; a.active = at
            result += line("carlink", "connected", a, at, summary(a, at, listOf(
                "ble_scan" to ("ble_scan_started" to "ble_peer_found"),
                "gatt" to ("gatt_connect_started" to "gatt_connected"),
                "ble_exchange" to ("ble_exchange" to "ble_exchange_done"),
                "wifi_join" to ("wifi_join" to "wifi_join_success"),
                "tcp" to ("tcp_connect_started" to "tcp_connected"),
                "auth" to ("tcp_connected" to "auth_success"),
                "negotiate" to ("state=NEGOTIATING" to "state=PLAYING"))))
        }
        if (event == "state=ENDED" && !a.terminal) {
            a.terminal = true
            val root = a.root ?: LinkEnd("session", a.reached, "local", "stopped")
            result += line("carlink", "ended", a, at, "played=${a.active?.let { elapsed(it, at) } ?: 0} reason=${a.reason} by=${root.by} cause=${root.cause} reached=${connectionToken(a.reached.lowercase().substringAfter("state="))}")
        }
        return result
    }

    private fun summary(a: Attempt, at: Long, phases: List<Pair<String, Pair<String, String>>>): String = buildString {
        append("total=${elapsed(a.start, at)}")
        for ((name, ends) in phases) {
            val start = a.marks[ends.first]; val end = a.marks[ends.second]
            if (start != null && end != null) append(" $name=${elapsed(start, end)}")
        }
        a.marks["first_video"]?.let { append(" first_video=${elapsed(a.start, it)}") }
    }
    private fun line(domain: String, event: String, a: Attempt, at: Long, detail: String): String {
        val text = "$domain $event attempt=${a.number} t=${elapsed(a.start, at)} dt=${elapsed(a.previous, at)}" +
            if (detail.isEmpty()) "" else " ${detail.replace('\n', ' ').replace('\r', ' ')}"
        a.previous = maxOf(a.previous, at)
        return text
    }
    private fun elapsed(from: Long, to: Long) = (to - from).coerceAtLeast(0)

    fun cast(message: String, connection: Long = 0, at: Long = clock()): List<String> {
        val end = diagnosticEnd(message, "renderer", "opening") ?: return emptyList()
        return linkEnded("cast", end, at, connection)
    }

    fun castEvent(event: String, detail: String, connection: Long, at: Long = clock()): List<String> =
        listOf(line("cast", event, castAttempt(connection, at), at, detail))

    private fun castAttempt(connection: Long, at: Long): Attempt = castAttempts.getOrPut(connection) {
        if (castAttempts.size >= 8) castAttempts.remove(castAttempts.keys.first())
        Attempt(++nextCast, at)
    }

    fun carlinkDiagnostic(message: String, connection: Long, at: Long = clock()): List<String> {
        if (diagnosticEnd(message, "session", "starting") == null) return emptyList()
        return carlink(message, at, connection)
    }

    private companion object {
        val carlinkMarks = listOf("link_start", "ble_scan_started", "ble_peer_found", "gatt_connect_started", "gatt_connected",
            "gatt_services_discovered", "ble_exchange", "ble_exchange_done", "wifi_join", "wifi_join_success", "tcp_connect_started",
            "tcp_connected", "auth", "auth_success", "state=NEGOTIATING", "state=PLAYING", "first_video")
            .withIndex().associate { it.value to it.index }
        val phoneMarks = listOf("starting", "discovering_mfi", "waiting_for_mfi", "requesting_mfi_permission", "mfi_ready",
            "starting_hotspot", "hotspot_ready", "waiting_for_paired_iphone", "discovering_iphone", "waiting_for_iphone",
            "connecting_bluetooth", "running_wireless", "requesting_iphone_permission", "waiting_for_reenumeration",
            "selecting_configuration", "opening_data_paths", "attaching_network", "pairing", "connecting_control", "running_control",
            "wireless_active_fallback", "wireless_active", "auth_setup_accepted", "pair_setup", "pair_verified", "setup_control",
            "setup_main_screen", "setup_cluster_screen", "record", "first_video_received", "first_video", "session_active")
            .withIndex().associate { it.value to it.index }
    }
}
