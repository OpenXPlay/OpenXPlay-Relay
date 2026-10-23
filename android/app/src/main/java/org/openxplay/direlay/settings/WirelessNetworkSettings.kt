// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-14
package org.openxplay.direlay.settings

import android.content.Context
import com.shilapi.xcertplay.AirPlayPersistence
import com.shilapi.xcertplay.orchestration.ManualHotspotBand
import com.shilapi.xcertplay.orchestration.ManualHotspotSecurity
import com.shilapi.xcertplay.orchestration.WirelessHotspotMode
import com.shilapi.xcertplay.orchestration.isManualHotspotChannelCompatible
import org.openxplay.direlay.R

enum class WirelessNetworkChoice(val mode: WirelessHotspotMode, val label: Int) {
    WIFI_DIRECT(WirelessHotspotMode.WIFI_P2P, R.string.ui_wifi_direct_recommended),
    PHONE_HOTSPOT(WirelessHotspotMode.MANUAL, R.string.direlay_car_hotspot),
    EXISTING_WIFI(WirelessHotspotMode.EXISTING_WIFI, R.string.direlay_existing_wifi);

    companion object {
        fun fromSavedName(name: String) = if (name == "CAR_HOTSPOT") PHONE_HOTSPOT else valueOf(name)
        // DiPlay no longer supports LOCAL_ONLY_HOTSPOT and stores it as MANUAL (phone hotspot).
        fun fromMode(mode: WirelessHotspotMode) =
            if (mode == WirelessHotspotMode.LOCAL_ONLY_HOTSPOT) PHONE_HOTSPOT else entries.first { it.mode == mode }
    }
}

data class WirelessNetworkSettings(
    val choice: WirelessNetworkChoice = WirelessNetworkChoice.WIFI_DIRECT,
    val manualSsid: String = "",
    val manualPassphrase: String = "",
    val band: ManualHotspotBand = ManualHotspotBand.AUTO,
    val channel: Int = 0,
    val security: ManualHotspotSecurity = ManualHotspotSecurity.OPEN,
    val sharedWifiNetworks: List<SharedWifiNetwork> = emptyList(),
) {
    override fun toString() = "WirelessNetworkSettings(choice=$choice, manualSsid=$manualSsid, manualPassphrase=[redacted], " +
        "band=$band, channel=$channel, security=$security, sharedWifiNetworks=$sharedWifiNetworks)"
    fun validate(allowEmptySharedWifi: Boolean = false, allowIncompleteHotspot: Boolean = false) {
        SharedWifiNetworks.validate(sharedWifiNetworks)
        when (choice) {
            WirelessNetworkChoice.EXISTING_WIFI -> require(allowEmptySharedWifi || sharedWifiNetworks.isNotEmpty())
            WirelessNetworkChoice.PHONE_HOTSPOT -> {
                if (allowIncompleteHotspot && manualSsid.isEmpty()) {
                    require((manualPassphrase.isEmpty() || manualPassphrase.length in 8..63) && '\u0000' !in manualPassphrase)
                } else validateCredentials(manualSsid, manualPassphrase)
                require(channel == 0 || isManualHotspotChannelCompatible(band, channel)) {
                    "Use channel 0 for automatic, or a channel supported by the selected band (1–14 for 2.4 GHz, 32–177 for 5 GHz)."
                }
                require(security == ManualHotspotSecurity.OPEN || manualPassphrase.length in 8..63) {
                    "Secured phone hotspots require an 8–63 character passphrase."
                }
            }
            WirelessNetworkChoice.WIFI_DIRECT -> Unit
        }
    }

    fun save(context: Context) {
        SharedWifiNetworks.validate(sharedWifiNetworks)
        AirPlayPersistence.saveWirelessHotspotMode(context, choice.mode)
        context.getSharedPreferences("direlay", Context.MODE_PRIVATE).edit().putBoolean("wirelessNetworkChosen", true)
            .putBoolean("wirelessNetworkInitialized", true).apply()
        AirPlayPersistence.saveManualHotspotSsid(context, manualSsid)
        AirPlayPersistence.saveManualHotspotPassphrase(context, manualPassphrase)
        AirPlayPersistence.saveManualHotspotBand(context, band)
        AirPlayPersistence.saveManualHotspotChannel(context, channel)
        AirPlayPersistence.saveManualHotspotSecurity(context, security)
        context.getSharedPreferences("direlay", Context.MODE_PRIVATE).edit()
            .putString("sharedWifiNetworks", SharedWifiNetworks.encode(sharedWifiNetworks)).apply()
    }

    companion object {
        fun validateCredentials(ssid: String, passphrase: String) = SharedWifiNetworks.validateCredentials(ssid, passphrase)

        fun load(context: Context): WirelessNetworkSettings {
            val relay = context.getSharedPreferences("direlay", Context.MODE_PRIVATE)
            val chosen = relay.getBoolean("wirelessNetworkChosen", false)
            val networks = SharedWifiNetworks.migrate(relay.getString("sharedWifiNetworks", null),
                AirPlayPersistence.loadExistingWifiSsid(context), AirPlayPersistence.loadExistingWifiPassphrase(context))
            if (!relay.contains("sharedWifiNetworks")) relay.edit().putString("sharedWifiNetworks", SharedWifiNetworks.encode(networks)).apply()
            return WirelessNetworkSettings(
                if (chosen) WirelessNetworkChoice.fromMode(AirPlayPersistence.loadWirelessHotspotMode(context)) else WirelessNetworkChoice.WIFI_DIRECT,
                AirPlayPersistence.loadManualHotspotSsid(context),
                AirPlayPersistence.loadManualHotspotPassphrase(context),
                AirPlayPersistence.loadManualHotspotBand(context),
                AirPlayPersistence.loadManualHotspotChannel(context),
                AirPlayPersistence.loadManualHotspotSecurity(context),
                networks,
            )
        }
    }
}
