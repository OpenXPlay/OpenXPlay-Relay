// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-15
package org.openxplay.direlay.admin

import com.shilapi.xcertplay.orchestration.ManualHotspotBand
import com.shilapi.xcertplay.orchestration.ManualHotspotSecurity
import org.openxplay.direlay.settings.*
import org.openxplay.direlay.ui.settingsSaveErrors

/** The only translation between the browser contract and Android's preference model. */
object AdminSettingsMapping {
    private fun choice(key: String, vararg values: String, visible: AdminVisibility? = null) =
        AdminField(key, "choice", options = values.map {
            AdminOption(it, if (key == "displayMode" && it == "relay-screen") "ui.relay" else "option.$key.$it", if (key in setOf("networkMode", "output", "wirelessDisplay")) "hint.option.$key.$it" else null,
                if (key == "networkMode") it else null)
        }, visibleWhen = visible)
    private fun visible(key: String, vararg values: String) = AdminVisibility(key, values.toList())
    private val fixed = visible("displayMode", "custom")
    val schema = listOf(
        AdminSection("output", listOf(choice("output", "web", "carlink", "wireless", "multiple"),
            choice("wirelessDisplay", "dlna", "miracast", visible = visible("output", "wireless")))),
        AdminSection("plugins", listOf(AdminField("plugins", "bool", hintKey = "hint.plugins"),
            AdminField("pluginsEnabled", "plugins"))),
        AdminSection("connection", listOf(choice("transport", "wireless", "usb"),
            AdminField("iphone", "phone", hintKey = "hint.field.iphone"))),
        AdminSection("network", listOf(choice("networkMode", "wifi-direct", "phone-hotspot", "shared-wifi", visible = visible("transport", "wireless")),
            AdminField("hotspotSsid", "text", visibleWhen = visible("networkMode", "phone-hotspot")),
            AdminField("hotspotPassword", "secret", visibleWhen = visible("networkMode", "phone-hotspot")),
            choice("hotspotBand", "auto", "2.4", "5", visible = visible("networkMode", "phone-hotspot")),
            AdminField("sharedWifiNetworks", "networks"))),
        AdminSection("display", listOf(choice("displayMode", "auto", "relay-screen", "custom"),
            AdminField("displayWidth", "number", visibleWhen = fixed, min = 320, max = 3840),
            AdminField("displayHeight", "number", visibleWhen = fixed, min = 240, max = 2160),
            AdminField("displayDensity", "number", visibleWhen = fixed, min = 120, max = 640))),
        AdminSection("audio", listOf(AdminField("carBluetoothAudio", "bool", hintKey = "hint.field.carBluetoothAudio"))),
        AdminSection("startup", listOf(AdminField("autoStart", "bool", hintKey = "hint.field.autoStart"), AdminField("bootStart", "bool"))),
        AdminSection("carlink", listOf(AdminField("carlinkName", "text", visibleWhen = visible("output", "carlink", "multiple")),
            AdminField("vehicleInformation", "bool", visibleWhen = visible("output", "carlink", "multiple")),
            AdminField("carlinkAutoReconnect", "bool", visibleWhen = visible("output", "carlink", "multiple")))),
    )
    fun output(s: RelaySettings) = when (OutputSelection.choice(s.outputs)) {
        OutputChoice.BROWSER -> "web"
        OutputChoice.CARLINK -> "carlink"
        OutputChoice.WIRELESS -> "wireless"
        null -> "multiple"
    }
    fun network(s: RelaySettings) = when (s.wirelessNetwork.choice) {
        WirelessNetworkChoice.WIFI_DIRECT -> "wifi-direct"
        WirelessNetworkChoice.PHONE_HOTSPOT -> "phone-hotspot"
        WirelessNetworkChoice.EXISTING_WIFI -> "shared-wifi"
    }
    fun values(s: RelaySettings, wireless: Boolean = s.connectionMode != ConnectionMode.USB, currentSsid: String? = null): Map<String, JsonElement> {
        val profile = s.display ?: CarDisplayFit.default
        return linkedMapOf("plugins" to s.plugins, "pluginsEnabled" to s.pluginsEnabled, "output" to output(s),
            "wirelessDisplay" to if (OutputSelection.wirelessKind(s.outputs) == WirelessDisplayKind.MIRACAST) "miracast" else "dlna",
            "transport" to if (wireless) "wireless" else "usb", "networkMode" to network(s),
            "hotspotSsid" to s.wirelessNetwork.manualSsid, "hotspotPassword" to s.wirelessNetwork.manualPassphrase,
            "hotspotBand" to when (s.wirelessNetwork.band) { ManualHotspotBand.AUTO -> "auto"; ManualHotspotBand.GHZ_2_4 -> "2.4"; ManualHotspotBand.GHZ_5 -> "5" },
            "sharedWifiNetworks" to s.wirelessNetwork.sharedWifiNetworks.map { mapOf("ssid" to it.ssid, "hasPassword" to it.passphrase.isNotEmpty(), "current" to (it.ssid == currentSsid)) },
            "iphone" to (s.iphoneAddress ?: "auto"), "displayMode" to when (s.displayMode) {
                DisplayMode.AUTOMATIC -> "auto"; DisplayMode.RELAY_SCREEN -> "relay-screen"; DisplayMode.FIXED -> "custom"
            }, "displayWidth" to profile.width, "displayHeight" to profile.height, "displayDensity" to profile.density,
            "carBluetoothAudio" to s.carBluetoothAudio, "autoStart" to s.autoStartWhenReady, "bootStart" to s.autoStartOnBoot,
            "carlinkName" to s.carlinkDeviceName, "vehicleInformation" to s.vehicleInformation, "carlinkAutoReconnect" to s.carlinkAutoReconnect)
    }
    private fun updatedNetworks(value: Any?, stored: List<SharedWifiNetwork>): List<SharedWifiNetwork> {
        require(value is List<*>)
        return value.map { item ->
            require(item is Map<*, *> && item.keys.all { it == "ssid" || it == "password" })
            val ssid = item["ssid"]
            require(ssid is String && '\r' !in ssid && '\n' !in ssid)
            val password = if (item.containsKey("password")) item["password"].also { require(it is String) } as String
                else requireNotNull(stored.firstOrNull { it.ssid == ssid }).passphrase
            SharedWifiNetwork(ssid, password)
        }.also(SharedWifiNetworks::validate)
    }
    data class Mapping(val settings: RelaySettings?, val errors: Map<String, String>)
    fun update(before: RelaySettings, changes: Map<String, JsonElement>, wireless: Boolean): Mapping {
        val current = values(before, wireless)
        val errors = linkedMapOf<String, String>()
        val fields = schema.flatMap { it.fields }.associateBy { it.key }
        changes.forEach { (key, value) ->
            val field = fields[key]
            val valid = when (field?.type) {
                "plugins" -> org.openxplay.direlay.plugins.RelayPlugins.validIds(value)
                "networks" -> runCatching { updatedNetworks(value, before.wirelessNetwork.sharedWifiNetworks) }.isSuccess
                "bool" -> value is Boolean
                "choice" -> value is String && field.options.orEmpty().any { it.value == value }
                "text", "secret", "phone" -> value is String
                "number" -> value is Number && value.toDouble().isFinite() && value.toDouble() == value.toInt().toDouble() &&
                    value.toInt() in (field.min ?: Int.MIN_VALUE)..(field.max ?: Int.MAX_VALUE)
                else -> false
            }
            if (!valid) errors[key] = if (field?.type == "plugins") "error.plugins" else if (field?.type == "number") "error.range" else if (field?.type == "networks") "error.networks" else "error.invalid"
        }
        if (errors.isNotEmpty()) return Mapping(null, errors)
        val v = current + changes
        fun string(key: String) = v[key] as String
        fun bool(key: String) = v[key] as Boolean
        fun number(key: String) = (v[key] as Number).toInt()
        val transportChanged = changes["transport"]?.let { it != current["transport"] } == true
        val networkChanged = changes.keys.any { it in listOf("networkMode", "hotspotSsid", "hotspotPassword", "hotspotBand", "sharedWifiNetworks") && changes[it] != current[it] }
        val profileChanged = changes.keys.any { it.startsWith("display") && changes[it] != current[it] }
        val displayMode = when (string("displayMode")) { "auto" -> DisplayMode.AUTOMATIC; "relay-screen" -> DisplayMode.RELAY_SCREEN; else -> DisplayMode.FIXED }
        val network = before.wirelessNetwork.copy(
            choice = when (string("networkMode")) { "wifi-direct" -> WirelessNetworkChoice.WIFI_DIRECT; "phone-hotspot" -> WirelessNetworkChoice.PHONE_HOTSPOT; else -> WirelessNetworkChoice.EXISTING_WIFI },
            manualSsid = string("hotspotSsid"), manualPassphrase = string("hotspotPassword"),
            band = when (string("hotspotBand")) { "auto" -> ManualHotspotBand.AUTO; "2.4" -> ManualHotspotBand.GHZ_2_4; else -> ManualHotspotBand.GHZ_5 },
            channel = if (changes["hotspotBand"] != null && changes["hotspotBand"] != current["hotspotBand"]) 0 else before.wirelessNetwork.channel,
            security = if (changes.containsKey("hotspotPassword") && changes["hotspotPassword"] != current["hotspotPassword"])
                if (string("hotspotPassword").isEmpty()) ManualHotspotSecurity.OPEN else ManualHotspotSecurity.WPA2 else before.wirelessNetwork.security,
            sharedWifiNetworks = if (changes.containsKey("sharedWifiNetworks")) updatedNetworks(changes["sharedWifiNetworks"], before.wirelessNetwork.sharedWifiNetworks) else before.wirelessNetwork.sharedWifiNetworks)
        if (changes["networkMode"] == "shared-wifi" && AdminSettingsMapping.network(before) != "shared-wifi" && network.sharedWifiNetworks.isEmpty())
            return Mapping(null, mapOf("networkMode" to "error.noSavedWifi"))
        val outputChanged = string("output") != current["output"] || string("wirelessDisplay") != current["wirelessDisplay"]
        if (string("output") == "multiple" && current["output"] != "multiple")
            return Mapping(null, mapOf("output" to "error.outputsOnPhone"))
        @Suppress("UNCHECKED_CAST")
        val pluginIds = v["pluginsEnabled"] as List<String>
        val candidate = before.copy(plugins = bool("plugins"), pluginsEnabled = pluginIds,
            connectionMode = if (!transportChanged) before.connectionMode else if (string("transport") == "wireless") ConnectionMode.WIRELESS else ConnectionMode.USB,
            wirelessNetwork = network, iphoneAddress = string("iphone").takeUnless { it == "auto" },
            displayMode = displayMode, display = if (!profileChanged) before.display else if (displayMode == DisplayMode.FIXED || before.display != null)
                DisplayProfile(number("displayWidth"), number("displayHeight"), number("displayDensity")) else null,
            carBluetoothAudio = bool("carBluetoothAudio"), autoStartWhenReady = bool("autoStart"), autoStartOnBoot = bool("bootStart"), carlinkDeviceName = string("carlinkName"), vehicleInformation = bool("vehicleInformation"), carlinkAutoReconnect = bool("carlinkAutoReconnect"))
        val choice = when (string("output")) {
            "web" -> OutputChoice.BROWSER
            "carlink" -> OutputChoice.CARLINK
            "wireless" -> OutputChoice.WIRELESS
            else -> null
        }
        val selected = if (outputChanged && choice != null) OutputSelection.choose(candidate, choice,
            if (string("wirelessDisplay") == "miracast") WirelessDisplayKind.MIRACAST else WirelessDisplayKind.DLNA,
            string("transport") == "wireless") else candidate
        if (CarlinkConnectionPolicy.usesWifiDirect(selected) && changes["networkMode"] == "wifi-direct")
            return Mapping(null, mapOf("networkMode" to if (selected.carlinkOutput)
                CarlinkConnectionPolicy.REASON_KEY else CarlinkConnectionPolicy.MIRACAST_REASON_KEY))
        val after = CarlinkConnectionPolicy.enabling(before, selected, string("transport") == "wireless")
        if (after.iphoneAddress != null && !after.iphoneAddress.matches(Regex("(?i)[0-9a-f]{2}(:[0-9a-f]{2}){5}"))) errors["iphone"] = "error.invalid"
        // Untouched, unconfigured network defaults must not block unrelated edits.
        val validateWireless = string("transport") == "wireless" && (networkChanged || transportChanged)
        settingsSaveErrors(after, validateWireless).keys.forEach { key ->
            val field = when (key) {
                "ssid" -> if (network.choice == WirelessNetworkChoice.PHONE_HOTSPOT) "hotspotSsid" else "sharedWifiNetworks"
                "password" -> if (network.choice == WirelessNetworkChoice.PHONE_HOTSPOT) "hotspotPassword" else "sharedWifiNetworks"
                "name" -> "carlinkName"
                else -> changes.keys.firstOrNull { it.startsWith("display") } ?: changes.keys.firstOrNull() ?: "displayMode"
            }
            errors[field] = "error.invalid"
        }
        if (errors.isEmpty()) try { after.validate(validateWireless, allowEmptySharedWifi = true) }
        catch (_: IllegalArgumentException) { errors[changes.keys.firstOrNull() ?: "displayMode"] = "error.invalid" }
        return Mapping(after.takeIf { errors.isEmpty() }, errors)
    }
}
