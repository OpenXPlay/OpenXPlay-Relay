// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-23
package org.openxplay.direlay.logs

/** Pure attempt accounting. The owner serializes calls; timestamps are captured at the event source. */
class ConnectionTimeline(private val clock: () -> Long, private val processStart: Long = clock()) {
    private class Attempt(val number: Long, val start: Long) {
        var previous = start
        var terminal = false
        var summarized = false
        var active: Long? = null
        var reason = "closed"
        var lastStatus: String? = null
        val marks = linkedMapOf<String, Long>()
    }
    private val attempts = mutableMapOf<String, Attempt>()
    private val transitions = linkedMapOf<String, String>()
    private val carlinkAttempts = linkedMapOf<Long, Attempt>()
    private var nextCarlink = 0L
    private var latestConnection = 0L

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
            status == "DiscoveringMfi" && old.lastStatus != "$status $detail"
        if (begin) attempts["phone"] = Attempt((old?.number ?: 0) + 1, at)
        val a = attempts.getValue("phone")
        if (a.lastStatus == "$status $detail") return emptyList()
        a.lastStatus = "$status $detail"
        a.marks.putIfAbsent(status, at)
        if (status == "Failed") { a.reason = detail; a.terminal = true }
        return listOf(line("phone", status, a, at, detail)) +
            if (status == "Failed") listOf(line("phone", "failed", a, at, detail)) else emptyList()
    }

    fun phone(event: String, detail: String = "", at: Long = clock()): List<String> {
        val a = attempts.getOrPut("phone") { Attempt(1, at) }
        if (event in setOf("first_video", "session_active", "session_ended") && a.marks.containsKey(event)) return emptyList()
        a.marks.putIfAbsent(event, at)
        if (event == "session_active") a.active = at
        if (event == "session_ended") a.terminal = true
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
        if (message.contains(" failed ")) a.reason = detail.substringAfter("reason=", detail).substringBefore(' ')
        if (event == "link_lost") a.reason = event
        val result = mutableListOf(line("carlink", event, a, at, detail))
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
            result += line("carlink", "ended", a, at, "played=${a.active?.let { elapsed(it, at) } ?: 0} reason=${a.reason}")
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
}
