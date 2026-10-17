// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-15
package org.openxplay.direlay.admin

import android.app.Application
import com.shilapi.xcertplay.orchestration.ManualHotspotBand
import com.shilapi.xcertplay.orchestration.ManualHotspotSecurity
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import org.openxplay.direlay.settings.*
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [28], manifest = Config.NONE, application = Application::class)
class AdminSettingsMappingTest {
    private fun initial(): RelaySettings {
        val context = RuntimeEnvironment.getApplication()
        listOf("direlay", "diplay", "xcertplay_airplay").forEach { context.getSharedPreferences(it, 0).edit().clear().commit() }
        return RelaySettingsStore(context).use { it.load() }
    }
    @Test fun allSupportedSettingsRoundTripWithoutLosingNativeFields() {
        val original = initial()
        for (network in WirelessNetworkChoice.entries) for (mode in DisplayMode.entries) for (output in listOf("web", "carlink", "both")) {
            val value = original.copy(connectionMode = ConnectionMode.WIRELESS,
                carlinkOutput = output != "web", bothOutputs = output == "both", requirePairingCode = true,
                iphoneAddress = "01:23:45:67:89:AB", autoStartOnBoot = true, autoStartWhenReady = false,
                carlinkDeviceName = "My car", vehicleInformation = true, displayMode = mode,
                display = if (mode == DisplayMode.FIXED) DisplayProfile(1920, 1080, 240) else null,
                wirelessNetwork = WirelessNetworkSettings(network, "Phone", "password", ManualHotspotBand.GHZ_5,
                    0, ManualHotspotSecurity.WPA2, listOf(SharedWifiNetwork("Home", "password"))))
            val result = AdminSettingsMapping.update(value, AdminSettingsMapping.values(value).filterKeys { it != "sharedWifiNetworks" }, true)
            if (network == WirelessNetworkChoice.WIFI_DIRECT && output != "web") {
                assertEquals(mapOf("networkMode" to CarlinkConnectionPolicy.REASON_KEY), result.errors)
                assertNull(result.settings)
            } else {
                assertTrue(result.errors.toString(), result.errors.isEmpty())
                assertEquals(value, result.settings)
            }
        }
        for (wireless in listOf(true, false)) assertEquals(original,
            AdminSettingsMapping.update(original, AdminSettingsMapping.values(original, wireless).filterKeys { it != "sharedWifiNetworks" }, wireless).settings)
        assertEquals(AdminSettingsMapping.schema.flatMap { it.fields }.map { it.key }.toSet(), AdminSettingsMapping.values(original).keys)
    }
    @Test fun partialUpdatesValidateTypesDimensionsAndCredentialsWithoutSaving() {
        val before = initial()
        assertEquals(before.copy(autoStartOnBoot = true), AdminSettingsMapping.update(before, mapOf("bootStart" to true), true).settings)
        listOf(mapOf("unknown" to true), mapOf("autoStart" to "true"), mapOf("displayWidth" to 800.5),
            mapOf("output" to "invalid"), mapOf("iphone" to "bad address"), mapOf("carlinkName" to ""),
            mapOf("displayMode" to "custom", "displayWidth" to 801),
            mapOf("networkMode" to "shared-wifi", "sharedWifiNetworks" to listOf(mapOf("ssid" to ""))),
            mapOf("networkMode" to "phone-hotspot", "hotspotSsid" to "Phone", "hotspotPassword" to "short"))
            .forEach { val result = AdminSettingsMapping.update(before, it, true); assertNull(result.settings); assertTrue(result.errors.isNotEmpty()) }
        val updated = AdminSettingsMapping.update(before, mapOf("displayMode" to "custom", "displayWidth" to 1024,
            "displayHeight" to 600, "displayDensity" to 160, "output" to "both", "iphone" to "auto"), true).settings!!
        assertEquals(DisplayProfile(1024, 600, 160), updated.display)
        assertTrue(updated.bothOutputs)
        assertTrue(updated.carlinkOutput)
        assertNull(updated.iphoneAddress)
    }
    @Test fun mappedValuesPersistThroughRelaySettingsStore() {
        val context = RuntimeEnvironment.getApplication()
        val before = initial()
        val result = AdminSettingsMapping.update(before, mapOf("transport" to "usb", "networkMode" to "shared-wifi",
            "sharedWifiNetworks" to listOf(mapOf("ssid" to "Home", "password" to "wifi-pass")), "output" to "carlink", "bootStart" to true), true).settings!!
        RelaySettingsStore(context).use { it.save(result) }
        RelaySettingsStore(context).use { assertEquals(result, it.load()) }
    }
    @Test fun sharedNetworkUpdatesNeverExposePasswordsAndKeepOmittedPasswords() {
        val before = initial().copy(wirelessNetwork = WirelessNetworkSettings(sharedWifiNetworks = listOf(
            SharedWifiNetwork("Home", "home-secret"), SharedWifiNetwork("Car", "car-secret"))))
        val values = AdminSettingsMapping.values(before, true, "Home")
        assertEquals(listOf(mapOf("ssid" to "Home", "hasPassword" to true, "current" to true),
            mapOf("ssid" to "Car", "hasPassword" to true, "current" to false)), values["sharedWifiNetworks"])
        assertFalse(values.toString().contains("home-secret"))
        assertFalse(values.containsKey("wifiPassword"))
        val next = AdminSettingsMapping.update(before, mapOf("sharedWifiNetworks" to listOf(
            mapOf("ssid" to "Home"), mapOf("ssid" to "Open", "password" to ""))), true).settings!!
        assertEquals(listOf(SharedWifiNetwork("Home", "home-secret"), SharedWifiNetwork("Open", "")), next.wirelessNetwork.sharedWifiNetworks)
        assertTrue(AdminSettingsMapping.update(before, mapOf("sharedWifiNetworks" to listOf(mapOf("ssid" to "Home", "password" to null))), true).errors.isNotEmpty())
    }
    @Test fun savedNetworksAreVisibleInBothModesAndSwitchRequiresASavedNetwork() {
        val field = AdminSettingsMapping.schema.flatMap { it.fields }.single { it.key == "sharedWifiNetworks" }
        assertEquals("networks", field.type)
        assertNull(field.visibleWhen)
        val before = initial().copy(wirelessNetwork = WirelessNetworkSettings(choice = WirelessNetworkChoice.WIFI_DIRECT, sharedWifiNetworks = emptyList()))
        val rejected = AdminSettingsMapping.update(before, mapOf("networkMode" to "shared-wifi"), true)
        assertNull(rejected.settings)
        assertEquals(mapOf("networkMode" to "error.noSavedWifi"), rejected.errors)
        val accepted = AdminSettingsMapping.update(before, mapOf("networkMode" to "shared-wifi",
            "sharedWifiNetworks" to listOf(mapOf("ssid" to "Open", "password" to ""))), true)
        assertTrue(accepted.errors.toString(), accepted.errors.isEmpty())
        assertEquals(WirelessNetworkChoice.EXISTING_WIFI, accepted.settings!!.wirelessNetwork.choice)
        val missingPassword = AdminSettingsMapping.update(before, mapOf("sharedWifiNetworks" to listOf(mapOf("ssid" to "New"))), true)
        assertEquals("error.networks", missingPassword.errors["sharedWifiNetworks"])
    }

}
