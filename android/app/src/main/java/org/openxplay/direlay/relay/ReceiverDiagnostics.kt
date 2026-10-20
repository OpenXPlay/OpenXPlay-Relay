// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-16
package org.openxplay.direlay.relay

/**
 * The latest receiver-info and a short window of per-second stats for each browser on the car
 * screen, kept as the browser's JSON text for the admin developer section. Every dimension is bounded.
 */
class ReceiverDiagnostics(private val clock: () -> Long = System::nanoTime) {
    data class Report(val client: String, val idleSeconds: Double, val info: String?, val stats: List<String>)
    private class Client(val id: String, var seen: Long, var info: String? = null, val stats: ArrayDeque<String> = ArrayDeque())
    data class FullscreenExit(val time: Long, val client: String, val json: String)
    private val exits = ArrayDeque<FullscreenExit>()
    @Synchronized fun fullscreenExits(): List<FullscreenExit> = exits.toList()
    private val clients = ArrayList<Client>()
    private val screenInfo = LinkedHashMap<ScreenDataKey, String>()
    /** Retain the last info for each recorded screen after the browser disconnects; never export client IDs. */
    @Synchronized fun latestInfo(screen: ScreenDataKey): org.json.JSONObject? = screenInfo[screen]?.let { org.json.JSONObject(it) }
    private fun rememberInfo(json: String) {
        runCatching {
            val info = org.json.JSONObject(json)
            val screen = info.getJSONObject("screen")
            val ratio = info.getDouble("devicePixelRatio")
            require(ratio.isFinite() && ratio > 0)
            val key = ScreenDataKey(Math.round(screen.getDouble("width") * ratio).toInt(),
                Math.round(screen.getDouble("height") * ratio).toInt(), Math.round(ratio * 160).toInt())
            require(listOf(key.width, key.height, key.densityDpi).all { it in 1..32768 })
            screenInfo.remove(key); screenInfo[key] = json
            if (screenInfo.size > 10) screenInfo.remove(screenInfo.keys.first())
        }
    }

    /** Keeps bounded web receiver reports and a separate recent history of involuntary fullscreen exits. */
    @Synchronized fun report(client: String, type: String, json: String) {
        if (type != "receiver-info" && type != "stats" && type != "fullscreen-exit") return
        if (client.isEmpty() || client.length > MAX_CLIENT_ID || client.any { it.isISOControl() }) return
        if (json.length > if (type == "receiver-info") MAX_INFO_CHARS else MAX_SAMPLE_CHARS) return
        if (type == "fullscreen-exit") {
            if (exits.size == MAX_SAMPLES) exits.removeFirst()
            exits.addLast(FullscreenExit(System.currentTimeMillis(), client, json)); return
        }
        val now = clock()
        expire(now)
        val entry = clients.firstOrNull { it.id == client } ?: Client(client, now).also { added ->
            // Make room by forgetting the browser that has been silent longest.
            if (clients.size >= MAX_CLIENTS) clients.remove(clients.minBy { it.seen })
            clients.add(added)
        }
        entry.seen = now
        if (type == "receiver-info") { entry.info = json; rememberInfo(json) }
        else {
            if (entry.stats.size == MAX_SAMPLES) entry.stats.removeAt(0)
            entry.stats.addLast(json)
        }
    }
    /** Browsers in arrival order, oldest sample first. */
    @Synchronized fun snapshot(): List<Report> {
        val now = clock()
        expire(now)
        return clients.map { Report(it.id, Math.round((now - it.seen) / 100_000_000.0) / 10.0, it.info, it.stats.toList()) }
    }
    /** Only combine diagnostics with the matching typed report; callbacks can arrive separately. */
    @Synchronized fun relayDroppedFrames(stats: org.openxplay.stream.core.OutputStats): Long? {
        val json = clients.firstOrNull { it.id == stats.origin.clientId }?.stats?.lastOrNull() ?: return null
        return runCatching {
            val value = org.json.JSONObject(json)
            if (value.getLong("rendered") != stats.rendered || value.getLong("dropped") != stats.dropped) null
            else value.getLong("relayDroppedFrames").takeIf { it >= 0 }
        }.getOrNull()
    }
    @Synchronized fun clear() { clients.clear(); exits.clear(); screenInfo.clear() }
    // The web output reports no disconnects; a browser silent this long has gone.
    private fun expire(now: Long) { clients.removeAll { now - it.seen >= EXPIRY_NANOS } }

    companion object {
        const val MAX_CLIENTS = 8
        const val MAX_SAMPLES = 30
        const val EXPIRY_NANOS = 60_000_000_000L
        const val MAX_INFO_CHARS = 65_536
        const val MAX_SAMPLE_CHARS = 4_096
        const val MAX_CLIENT_ID = 64
    }
}
