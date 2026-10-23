// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-23
package org.openxplay.direlay.carlinkdebug

import org.openxplay.direlay.logs.BoundedLogWorker
import org.openxplay.stream.clink.trace.*

/** Baseline selection happens before enqueueing. Stream's hot events never reach the worker. */
class CarlinkSummaryTrace(private val mirror: (String) -> Unit,
    private val clean: (String) -> String = { it }, private val clock: () -> Long = { System.nanoTime() / 1_000_000 },
    private val timeline: (String) -> Unit = {}, private val car: (String) -> Unit = {},
    private val emitEvents: Boolean = true) : ClinkTrace, AutoCloseable {
    private val worker = BoundedLogWorker("carlink-summary", warning = { mirror("logging $it") })
    private val selection = Selection(clock)
    private val identity = linkedMapOf<String, String>()
    private var summarized = false
    private var playing = false
    private var firstVideo = false
    private val firstVideoReported = java.util.concurrent.atomic.AtomicBoolean()
    private var lastView: String? = null
    private var lastWindow: String? = null

    override fun record(event: ClinkTraceEvent) {
        if (!eligible(event)) return
        val at = clock()
        runCatching { worker.offer {
            val selected = selection.select(event, at)
            // Plain diagnostics (fields = detail only) are already in DiRelayCarlink and the timeline, with timings.
            val repeated = event is ClinkTraceEvent.Link && event.fields.keys == setOf("detail")
            if (emitEvents && selected != null && !repeated) mirror(clean(CarlinkTraceRecorder.encode(event, selectedAtMillis = null)
                .let { if (selected > 0) it.dropLast(1) + ",\"suppressed\":$selected}" else it }))
            if (event is ClinkTraceEvent.Link && event.fields["detail"] == "state=ENDED") {
                val suppressed = selection.takeSuppressed()
                if (emitEvents && suppressed > 0) mirror(clean(CarlinkTraceRecorder.encode(
                    ClinkTraceEvent.Video(event.atNanos, event.session, "keyframe_request_suppressed"), selectedAtMillis = null)
                    .dropLast(1) + ",\"suppressed\":$suppressed}"))
            }
            observe(event)
        } }
    }
    private fun observe(event: ClinkTraceEvent) {
        when (event) {
            is ClinkTraceEvent.Link -> when (event.stage) {
                "link_start" -> { identity.clear(); summarized = false; playing = false; firstVideo = false; lastView = null; lastWindow = null }
                "ble_advertisement", "ble_reply_name" -> event.fields["name"]?.let { identity["name"] = it }
                "head_unit_certificate" -> {
                    identity["vendor"] = event.fields["vendor_name"].orEmpty()
                    identity["product"] = event.fields["product_name"].orEmpty()
                }
                "wifi_handoff" -> event.fields["ssid"]?.let { identity["ssid"] = it }
                "ble_exchange_done" -> event.fields["detail"]?.substringAfter("frequency=", "")?.let { identity["frequency"] = it }
                "hevc_m4" -> identity["hevc"] = event.fields["peer"] ?: "unknown"
                "state" -> {
                    val state = event.fields["detail"]
                    if (state == "state=PLAYING") { playing = true }
                    if (state == "state=ENDED") { if (!summarized) summary(); playing = false }
                }
            }
            is ClinkTraceEvent.Negotiation -> {
                identity["mode"] = event.mode; identity["audio"] = event.audio.toString()
                identity["codec"] = event.decoders.joinToString(",")
            }
            is ClinkTraceEvent.Rtsp -> if (event.direction == "RX") {
                for (line in event.body.lineSequence()) {
                    val key = line.substringBefore(':').trim()
                    val value = line.substringAfter(':', "").trim()
                    if (key == "wfd_uibc_capability") identity["uibc"] = value
                    if (key == "iccoa_support_hevc") identity["hevc"] = value
                }
            }
            is ClinkTraceEvent.Frame -> if (event.direction == "RX" && event.service == "control" && event.method in setOf(24, 25)) {
                fun number(id: Int) = (event.fields[id] as? ClinkTraceField.Number)?.value
                val width = number(2); val height = number(3)
                if (width != null && height != null) window("${width}x$height")
            }
            is ClinkTraceEvent.Video -> if (event.event == "format") {
                event.codec?.let { identity["codec"] = it }
                if (event.width != null && event.height != null) identity["stream"] = "${event.width}x${event.height}"
            }
            else -> Unit
        }
    }
    fun view(width: Int, height: Int) { runCatching { worker.offer {
        val value = "${width}x$height"
        identity["view"] = value
        if (lastView != null && lastView != value) change("view_changed", "view=$value")
        lastView = value
    } } }
    private fun window(value: String) {
        identity["window"] = value
        if (lastWindow != null && lastWindow != value) change("window_changed", "window=$value")
        lastWindow = value
    }
    fun video(codec: String, width: Int, height: Int, liveFrames: Long) { runCatching {
        if (liveFrames > 0 && firstVideoReported.compareAndSet(false, true))
            timeline("first_video codec=$codec size=${width}x$height source=statistics")
        worker.offer {
        if (width > 0 && height > 0) { identity["stream"] = "${width}x$height"; identity["codec"] = codec }
        if (liveFrames > 0) firstVideo = true
        if (playing && firstVideo) summary()
    } } }
    private fun change(event: String, detail: String) {
        mirror(clean("carlink $event $detail")); timeline("$event $detail")
    }
    private fun summary() {
        if (summarized) return
        summarized = true
        // Nothing was learned about a car (for example a BLE scan timeout): the failure lines say enough.
        if (identity.values.none { it.isNotBlank() }) return
        val detail = listOf("vendor", "product", "name", "ssid", "frequency", "mode", "view", "window", "stream", "codec", "hevc", "audio", "uibc")
            .joinToString(" ") { "$it=${quote(identity[it] ?: "unknown")}" }
        mirror(clean("carlink car $detail")); car(detail)
    }
    private fun quote(value: String) = "\"${value.replace("\\", "\\\\").replace("\"", "\\\"").replace('\n', ' ').replace('\r', ' ')}\""
    internal fun awaitIdle() = worker.awaitIdle()
    override fun close() = worker.close()

    /** Called on one worker; per-session limits use an injected monotonic clock. */
    class Selection(private val clock: () -> Long) {
        private var session = Long.MIN_VALUE
        private var mediaAt: Long? = null
        private var keyframeAt: Long? = null
        private var suppressed = 0L
        fun select(event: ClinkTraceEvent, now: Long = clock()): Long? {
            if (!eligible(event)) return null
            if (event.session > 0 && event.session != session) { session = event.session; mediaAt = null; keyframeAt = null; suppressed = 0 }
            if (event is ClinkTraceEvent.Media) {
                if (mediaAt?.let { now - it < 10_000 } == true) return null
                mediaAt = now
            }
            if (event is ClinkTraceEvent.Video && event.event == "keyframe_request") {
                if (keyframeAt?.let { now - it < 1000 } == true) { suppressed++; return null }
                keyframeAt = now
                return suppressed.also { suppressed = 0 }
            }
            return 0
        }
        fun takeSuppressed(): Long = suppressed.also { suppressed = 0 }
    }
    companion object {
        private val hotVideo = setOf("live", "reanchor", "clock", "codec_mismatch")
        private val controlMethods = setOf(2, 3, 4, 5, 6, 9, 15, 16, 23, 24, 25)
        fun eligible(event: ClinkTraceEvent): Boolean = when (event) {
            is ClinkTraceEvent.Input -> false
            is ClinkTraceEvent.Video -> event.event !in hotVideo
            // Method 1 is the periodic heartbeat; method 12 carries input keys. Category 7 is ACK.
            is ClinkTraceEvent.Frame -> event.service == "control" && event.category == 1 &&
                event.method in controlMethods
            else -> true
        }
    }
}
