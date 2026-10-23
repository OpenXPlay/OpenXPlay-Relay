// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-23
package org.openxplay.direlay.logs

import org.json.JSONObject
import org.openxplay.stream.core.*

/** Existing ports report observations, not socket lifecycle. No timeout is called a disconnect. */
class BrowserTimeline(private val log: ConnectionLog) : AutoCloseable {
    private val worker = BoundedLogWorker("browser-timeline")
    private data class Client(var info: String? = null, var rendered: Boolean = false)
    private val clients = linkedMapOf<String, Client>()
    private var capabilities: OutputCapabilities? = null
    fun report(client: String, type: String, json: String) {
        if (client.length > 64 || json.length > 65_536 || type !in setOf("receiver-info", "relay-close", "stats")) return
        runCatching { worker.offer {
            val entry = clients.getOrPut(client) {
                if (clients.size >= 8) clients.remove(clients.keys.first())
                log.event("browser", "client_observed", "client=$client source=$type address=unavailable")
                Client()
            }
            when (type) {
                "receiver-info" -> {
                    if (entry.info == json) return@offer
                    entry.info = json
                    val info = JSONObject(json)
                    val ua = info.optString("userAgent")
                    val family = when {
                        "Edg/" in ua -> "Edge"; "Chrome/" in ua -> "Chrome"; "Firefox/" in ua -> "Firefox"
                        "Safari/" in ua -> "Safari"; else -> "unknown"
                    }
                    val screen = info.optJSONObject("screen")
                    val decoders = info.optJSONArray("videoDecode")
                    val codecs = if (decoders == null) "unavailable" else (0 until minOf(decoders.length(), 32)).mapNotNull { i ->
                        decoders.optJSONObject(i)?.let { d ->
                            "${d.optString("codec").take(64)}:${d.optInt("width")}x${d.optInt("height")}:" +
                                "hw=${d.optBoolean("preferHardware")}:sw=${d.optBoolean("preferSoftware")}:mse=${d.optBoolean("mediaSource")}:" +
                                "fps30=${d.optJSONObject("fps30")?.optBoolean("supported")}:fps60=${d.optJSONObject("fps60")?.optBoolean("supported") }"
                        }
                    }.joinToString(",")
                    val detail = "client=$client ua=$family screen=${screen?.optInt("width")}x${screen?.optInt("height")} " +
                        "videoDecoder=${info.optBoolean("videoDecoder")} codecs=$codecs source=receiver-info"
                    log.event("browser", "receiver_info", detail)
                }
                "relay-close" -> { log.event("browser", "client_disconnected", "client=$client source=relay-close reason=$json"); clients.remove(client) }
                "stats" -> {
                    val stats = JSONObject(json)
                    if (!entry.rendered && stats.optLong("rendered") > 0) {
                        entry.rendered = true
                        log.event("browser", "first_render_observed", "client=$client source=stats")
                    }
                }
            }
        } }
    }
    fun capabilities(value: OutputCapabilities) { runCatching { worker.offer {
        if (capabilities == value) return@offer
        capabilities = value
        val screen = value.displays[ScreenId.MAIN]
        if (screen == null && value.decoders.isEmpty() && value.audio.isEmpty()) return@offer
        log.event("browser", "capabilities", "clients=aggregate screen=${screen?.widthPx}x${screen?.heightPx} " +
            "window=${screen?.viewport?.width}x${screen?.viewport?.height} decoders=${value.decoders.joinToString { it.codecString }} audio=${value.audio.joinToString()}")
    } } }
    override fun close() = worker.close()
    internal fun awaitIdle() = worker.awaitIdle()
}

class TimelineDisplaySettingsPort(private val port: org.openxplay.stream.web.DisplaySettingsPort,
    private val log: ConnectionLog) : org.openxplay.stream.web.DisplaySettingsPort by port {
    override fun set(settings: org.openxplay.stream.web.DisplaySettings, origin: Origin) {
        port.set(settings, origin)
        log.transition("browser", "display-settings/${origin.clientId}", "display_settings",
            "client=${origin.clientId} resolution=${settings.resolution} screenWidth=${settings.screenWidth} videoCodec=${settings.videoCodec}")
    }
    override fun listen(callback: (org.openxplay.stream.web.DisplaySettings) -> Unit): AutoCloseable = port.listen { settings ->
        log.transition("browser", "display-settings-published", "display_settings_published",
            "clients=aggregate resolution=${settings.resolution} screenWidth=${settings.screenWidth} videoCodec=${settings.videoCodec}")
        callback(settings)
    }
}

/** Only observes the host port; media delivery stays delegated. */
class TimelineWebOutput(private val output: ProjectionOutput, private val observer: BrowserTimeline) : ProjectionOutput by output {
    override fun open(host: OutputHost) = output.open(object : OutputHost by host {
        override fun capabilities(capabilities: OutputCapabilities) {
            runCatching { observer.capabilities(capabilities) }
            host.capabilities(capabilities)
        }
    })
}
