// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-15
package org.openxplay.direlay.admin

import org.json.JSONArray
import org.json.JSONObject

/** JSON values use the platform JSON implementation; no serialization runtime is required. */
typealias JsonElement = Any?
interface RelayAdmin {
    fun responseSent() {}
    fun responseFailed() {}
    fun status(): AdminStatus
    fun settings(): AdminSettings
    fun update(values: Map<String, JsonElement>): AdminUpdate
    fun action(name: String): AdminResult
    fun phones(): List<AdminPhone>
    /** Latest receiver-info and recent stats per browser, for `GET /status?diagnostics=1`. */
    fun receivers(): List<AdminReceiver> = emptyList()
}
interface AdminJson { fun json(): JSONObject }
internal fun obj(vararg values: Pair<String, Any?>): JSONObject = JSONObject().apply {
    values.forEach { (key, value) -> put(key, jsonValue(value)) }
}
internal fun jsonValue(value: Any?): Any = when (value) {
    null -> JSONObject.NULL
    is AdminJson -> value.json()
    is Map<*, *> -> JSONObject().apply { value.forEach { (k, v) -> put(k.toString(), jsonValue(v)) } }
    is Iterable<*> -> JSONArray().apply { value.forEach { put(jsonValue(it)) } }
    else -> value
}
data class AdminIphone(val name: String?, val reason: String) : AdminJson {
    override fun json() = obj("name" to name, "reason" to reason)
}
data class AdminNetwork(val mode: String, val wifi: String?) : AdminJson {
    override fun json() = obj("mode" to mode, "wifi" to wifi)
}
data class AdminCarlink(val state: String, val detail: String) : AdminJson {
    override fun json() = obj("state" to state, "detail" to detail)
}
data class AdminHostAddress(val interfaceName: String, val ipv4: String, val developerOnly: Boolean) : AdminJson {
    override fun json() = obj("interface" to interfaceName, "ipv4" to ipv4, "developerOnly" to developerOnly)
}
data class AdminHotspot(val ssid: String, val password: String, val ip: String?, val active: Boolean) : AdminJson {
    override fun json() = obj("ssid" to ssid, "password" to password, "ip" to ip, "active" to active)
}
data class AdminPairedDevice(val name: String, val connected: Boolean, val iphone: Boolean) : AdminJson {
    override fun json() = obj("name" to name, "connected" to connected, "iphone" to iphone)
}
/** One browser's diagnostics; the reports are the browser's own JSON without the message type. */
data class AdminReceiver(val client: String, val idleSeconds: Double, val info: String?, val stats: List<String>) : AdminJson {
    override fun json() = obj("client" to client, "idleSeconds" to idleSeconds, "info" to info?.let(::report),
        "stats" to JSONArray().apply { stats.mapNotNull(::report).forEach { put(it) } })
    private fun report(text: String): JSONObject? = try { JSONObject(text).apply { remove("type") } } catch (_: org.json.JSONException) { null }
}
data class AdminStatus(val platform: String, val name: String, val version: String,
    val host: String, val adminUrl: String, val receiverUrls: List<String>, val state: String,
    val running: Boolean, val autoStartPaused: Boolean, val iphone: AdminIphone,
    val network: AdminNetwork, val output: String, val carlink: AdminCarlink?, val notices: List<String>,
    val hostAddresses: List<AdminHostAddress> = emptyList(), val receiverPort: Int = 8443,
    val adminPort: Int = 6975, val pairingAvailable: Boolean = false, val pairingDeadline: Long? = null,
    val hotspot: AdminHotspot? = null, val wifiControl: Boolean = false,
    val video: Map<String, Any?> = mapOf("codec" to "H264", "reason" to "no-main-screen-output"),
    val pairingRemaining: Long? = null, val pairingNeedsConfirmation: Boolean = false, val pairingOnRelay: Boolean = false,
    val pairedDevices: List<AdminPairedDevice> = emptyList(), val receivers: List<AdminReceiver>? = null,
    val pairingCancelable: Boolean = false, val pairingNeedsPermission: Boolean = false,
    val pairingPermissionDenied: Boolean = false, val pairingNotified: Boolean = false) : AdminJson {
    override fun json() = obj("platform" to platform, "name" to name, "version" to version,
        "hostAddresses" to hostAddresses, "receiverPort" to receiverPort, "adminPort" to adminPort,
        "pairingAvailable" to pairingAvailable, "pairingDeadline" to pairingDeadline,
        // Device names appear only in this authenticated response, never in logs.
        "pairingRemaining" to pairingRemaining, "pairingCancelable" to pairingCancelable,
        "pairingNeedsConfirmation" to pairingNeedsConfirmation, "pairingOnRelay" to pairingOnRelay, "pairedDevices" to pairedDevices,
        "pairingNeedsPermission" to pairingNeedsPermission, "pairingPermissionDenied" to pairingPermissionDenied,
        "pairingNotified" to pairingNotified,
        "hotspot" to hotspot, "wifiControl" to wifiControl, "video" to video,
        "host" to host, "adminUrl" to adminUrl, "receiverUrls" to receiverUrls, "state" to state,
        "running" to running, "autoStartPaused" to autoStartPaused, "iphone" to iphone,
        "network" to network, "output" to output, "carlink" to carlink, "notices" to notices).apply {
        receivers?.let { put("receivers", jsonValue(it)) }
    }
}
data class AdminOption(val value: String, val labelKey: String, val hintKey: String? = null,
    val illustration: String? = null) : AdminJson {
    override fun json() = obj("value" to value, "labelKey" to labelKey).apply {
        hintKey?.let { put("hintKey", it) }; illustration?.let { put("illustration", it) }
    }
}
data class AdminVisibility(val key: String, val equals: List<String>) : AdminJson {
    override fun json() = obj("key" to key, "equals" to equals)
}
data class AdminField(val key: String, val type: String, val labelKey: String = "field.$key",
    val hintKey: String? = null, val options: List<AdminOption>? = null,
    val visibleWhen: AdminVisibility? = null, val min: Int? = null, val max: Int? = null) : AdminJson {
    override fun json() = obj("key" to key, "type" to type, "labelKey" to labelKey).apply {
        hintKey?.let { put("hintKey", it) }; options?.let { put("options", jsonValue(it)) }
        visibleWhen?.let { put("visibleWhen", it.json()) }; min?.let { put("min", it) }; max?.let { put("max", it) }
    }
}
data class AdminSection(val id: String, val fields: List<AdminField>, val titleKey: String = "section.$id") : AdminJson {
    override fun json() = obj("id" to id, "titleKey" to titleKey, "fields" to fields)
}
data class AdminSettings(val schema: List<AdminSection>, val values: Map<String, JsonElement>,
    val installedPlugins: List<org.openxplay.direlay.plugins.InstalledPlugin> = emptyList()) : AdminJson {
    override fun json() = obj("schema" to schema, "values" to values, "installedPlugins" to installedPlugins)
}
data class AdminUpdate(val applied: String = "now", val errors: Map<String, String> = emptyMap()) : AdminJson {
    override fun json() = if (errors.isEmpty()) obj("ok" to true, "applied" to applied) else obj("errors" to errors)
}
data class AdminResult(val error: String? = null) : AdminJson {
    override fun json() = if (error == null) obj("ok" to true) else obj("error" to error)
}
data class AdminPhone(val id: String, val name: String, val kind: String, val connected: Boolean,
    val likelyIphone: Boolean, val selected: Boolean) : AdminJson {
    override fun json() = obj("id" to id, "name" to name, "kind" to kind, "connected" to connected,
        "likelyIphone" to likelyIphone, "selected" to selected)
}

internal fun readJsonValue(value: Any?): Any? = when (value) {
    JSONObject.NULL -> null
    is JSONObject -> value.keys().asSequence().associateWith { readJsonValue(value.get(it)) }
    is JSONArray -> (0 until value.length()).map { readJsonValue(value.get(it)) }
    else -> value
}
