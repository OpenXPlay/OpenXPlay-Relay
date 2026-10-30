// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-30
package org.openxplay.direlay.logs

/** Consume fixed diagnostic fields, never platform exception text or peer identity. */
internal fun diagnosticEnd(message: String, defaultLink: String, reached: String): LinkEnd? {
    val fields = Regex("(?:^| )([A-Za-z][A-Za-z0-9]*)=([^ ]+)").findAll(message)
        .associate { it.groupValues[1] to it.groupValues[2] }
    if (fields["by"] == null || fields["cause"] == null) return null
    val stage = message.substringBefore(' ')
    val link = fields["link"] ?: when (stage) {
        "ble_scan" -> "ble"; "ble_exchange", "gatt_connect", "gatt_connection", "gatt_disconnect" -> "gatt"
        "wifi_join", "link_lost" -> "wifi"; "tcp_connect" -> "tcp"
        else -> stage.takeIf { it in setOf("ble", "gatt", "wifi", "tcp", "session", "renderer") } ?: defaultLink
    }
    val detail = listOfNotNull(fields["exception"], fields["timeout"]?.let { "timeout=$it" }, fields["limitMs"]?.let { "limitMs=$it" }).joinToString(" ")
    return LinkEnd(link, fields["reached"] ?: reached,
        if (fields["by"] == "relay") "local" else fields.getValue("by"), fields.getValue("cause"),
        detail = detail,
        age = fields["age"]?.toLongOrNull() ?: -1, idle = fields["idle"]?.toLongOrNull() ?: -1,
        facts = fields.filterKeys { it in setOf("status", "code", "service") }).filtered()
}

internal fun carlinkEnd(message: String, reached: String): LinkEnd? {
    diagnosticEnd(message, "session", reached)?.let { return it }
    if (message == "link_lost" || message == "link_stop") return null
    val stage = message.substringBefore(' ')
    val code = message.substringAfter("reason=", "").substringBefore(' ')
    val link = when (stage) {
        "ble_scan" -> "ble"; "ble_exchange", "gatt_connect", "gatt_connection", "gatt_closed" -> "gatt"
        "wifi_join", "link_lost", "network_request_released" -> "wifi"
        "tcp_connect", "session", "link_stop" -> "session"
        else -> return null
    }
    if (" failed " !in message && stage !in setOf("link_lost", "gatt_closed", "network_request_released", "link_stop") &&
        !(stage == "gatt_connection" && "state=disconnected" in message)) return null
    val cause = when {
        stage in setOf("gatt_closed", "network_request_released", "link_stop") -> "teardown"
        code == "wifi_off" -> "radio_off"
        stage == "link_lost" || code == "wifi_link_lost" || code == "link_lost" -> "network_lost"
        code.endsWith("timeout") -> if (code == "heartbeat_timeout") "health_timeout" else "read_timeout"
        code in setOf("auth_closed", "vehicle_channel_closed") || stage == "gatt_connection" -> "peer_close"
        else -> "connect_failed"
    }
    val by = when (cause) { "network_lost", "radio_off" -> "os"; "read_timeout", "health_timeout" -> "timeout"; "peer_close" -> "peer"; else -> "local" }
    return LinkEnd(link, reached, by, cause, detail = code).filtered()
}
