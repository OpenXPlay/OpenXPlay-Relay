// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-30
package org.openxplay.direlay.logs

/** Relay's filtered observation; upstream objects never enter the accounting worker. */
data class LinkEnd(val link: String, val reached: String, val by: String, val cause: String,
    val detail: String = "", val age: Long = -1, val idle: Long = -1,
    val facts: Map<String, String> = emptyMap()) {
    fun filtered(redact: (String) -> String = { it }): LinkEnd = copy(
        link = link.takeIf { it in factKeys } ?: "unknown", reached = connectionToken(reached),
        by = by.takeIf { it in setOf("peer", "local", "timeout", "os") } ?: "local",
        cause = connectionToken(cause), detail = connectionDetail(redact(detail)),
        facts = facts.entries.asSequence().filter { it.key in factKeys[link].orEmpty() }
            .mapNotNull { (key, value) -> value.takeIf { it.length <= 80 && factValue.matches(it) && !mac.containsMatchIn(it) && !hex.containsMatchIn(it) }
                ?.let { key to it } }.take(12).toMap())

    fun line(): String = "link=$link reached=$reached by=$by cause=$cause age=${age.coerceAtLeast(-1)} idle=${if (idle < 0) "-" else idle}" +
        facts.entries.joinToString("") { " ${it.key}=${it.value}" } + quotedDetail(detail)

    private companion object {
        val factValue = Regex("[A-Za-z0-9_.,:/+\\[\\]-]{1,80}")
        val mac = Regex("(?i)[0-9a-f]{2}(?::[0-9a-f]{2}){5}")
        val hex = Regex("[A-Fa-f0-9]{16,}")
        val factKeys = mapOf(
            "bluetooth" to setOf("attempt", "consecutiveFailures", "discovering", "bondState", "aclConnected", "cachedIap2Service", "elapsedMs"),
            "iap2_bootstrap" to setOf("iap2State", "wirelessCarPlayAvailable", "transportIdentifier", "carPlayStartSessions", "postTransportConfigs", "handoffRequested", "tunnelReady", "wirelessActive"),
            "airplay_control" to setOf("connection", "requests", "lastRequest", "lastStatus", "rxAgoMs", "txAgoMs", "pairSetup", "pairVerified", "verifyOnly", "record", "streams"),
            "airplay_event" to setOf("type", "bytes", "packets", "rxAgoMs"),
            "stream_video" to setOf("type", "bytes", "packets", "rxAgoMs"),
            "stream_audio" to setOf("type", "bytes", "packets", "rxAgoMs"),
            "stream_other" to setOf("type", "bytes", "packets", "rxAgoMs", "channel"),
            "runtime_iap2" to setOf("bytes", "packets", "rxAgoMs"),
            "wireless_run" to setOf("next", "nextInMs", "generation"),
            "usb" to setOf("stage", "next", "nextInMs"),
            "web_client" to setOf("client", "code", "txStalledMs", "videoFramesSent", "droppedFrames", "bytesSent"),
            "ble" to setOf("status"), "gatt" to setOf("status"), "wifi" to setOf("status"),
            "tcp" to setOf("code", "service"), "session" to emptySet(), "renderer" to setOf("code", "status", "service"))
    }
}

internal fun connectionToken(value: String, fallback: String = "unknown") =
    value.takeIf { it.length in 1..64 && it.all { c -> c in 'a'..'z' || c in '0'..'9' || c == '_' } } ?: fallback

internal fun connectionDetail(value: String): String = Regex("[A-Fa-f0-9]{16,}").replace(
    Regex("(?i)[0-9a-f]{2}(?::[0-9a-f]{2}){5}").replace(
        Regex("""(?i)\b(?:ssid|deviceName|device_name|passphrase|password|pin|psk|token|key|pairing)\s*[:=]\s*("[^"]*"|'[^']*'|[^ ,;]+)""")
            .replace(value) { it.value.substringBefore('=').substringBefore(':') + "=[redacted]" }
            .map { if (it in ' '..'~') it else ' ' }.joinToString(""), "[redacted]"), "[redacted]")
    .replace('"', '\'').replace('\\', '/').trim().take(160)

internal fun quotedDetail(value: String) = if (value.isBlank()) "" else " detail=\"${connectionDetail(value)}\""
