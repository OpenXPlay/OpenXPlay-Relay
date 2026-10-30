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
    @Test fun highPerformanceStaysNativeAndSurvivesAdminUpdates() {
        assertFalse(AdminSettingsMapping.schema.flatMap { it.fields }.any { it.key == "highPerformance" })
        for (highPerformance in listOf(false, true)) {
            val before = initial().copy(highPerformance = highPerformance)
            assertFalse(AdminSettingsMapping.values(before).containsKey("highPerformance"))
            assertEquals(before.copy(autoStartOnBoot = true),
                AdminSettingsMapping.update(before, mapOf("bootStart" to true), true).settings)
            assertNull(AdminSettingsMapping.update(before, mapOf("highPerformance" to true), true).settings)
        }
    }

    private fun initial(): RelaySettings {
        val context = RuntimeEnvironment.getApplication()
        listOf("direlay", "diplay", "xcertplay_airplay").forEach { context.getSharedPreferences(it, 0).edit().clear().commit() }
        return RelaySettingsStore(context).use { it.load() }
    }
    @Test fun teslaSettingsRemainNativeAndSurviveUnrelatedAdminEdits() {
        val before = initial().copy(teslaBrowser = TeslaBrowserMode.PUBLIC_ADDRESS,
            teslaVirtualAddress = "100.88.0.2", teslaPublicAddress = "3.3.3.2",
            teslaMirrorAddress = "100.88.0.3", teslaAndroidAddress = "3.3.3.3")
        assertFalse(AdminSettingsMapping.schema.flatMap { it.fields }.any { it.key.startsWith("tesla") })
        assertFalse(AdminSettingsMapping.values(before).keys.any { it.startsWith("tesla") })
        assertEquals(before.copy(autoStartOnBoot = true),
            AdminSettingsMapping.update(before, mapOf("bootStart" to true), true).settings)
        for (key in listOf("teslaBrowser", "teslaVirtualAddress", "teslaPublicAddress", "teslaMirrorAddress", "teslaAndroidAddress"))
            assertNull(AdminSettingsMapping.update(before, mapOf(key to "3.3.3.3"), true).settings)
    }
    @Test fun allSupportedSettingsRoundTripWithoutLosingNativeFields() {
        val original = initial().copy(videoStats = VideoStatsWindow.SECONDS_15)
        for (network in WirelessNetworkChoice.entries) for (mode in DisplayMode.entries) for (outputs in RelayOutput.entries.map { setOf(it) } + listOf(RelayOutput.entries.toSet())) {
            val value = original.copy(outputs = outputs, connectionMode = ConnectionMode.WIRELESS,
                iphoneAddress = "01:23:45:67:89:AB", autoStartOnBoot = true, autoStartWhenReady = false,
                carlinkDeviceName = "My car", vehicleInformation = true, carBluetoothAudio = true, displayMode = mode,
                display = if (mode == DisplayMode.FIXED) DisplayProfile(1920, 1080, 240) else null,
                wirelessNetwork = WirelessNetworkSettings(network, "Phone", "password", ManualHotspotBand.GHZ_5,
                    0, ManualHotspotSecurity.WPA2, listOf(SharedWifiNetwork("Home", "password"))))
            val result = AdminSettingsMapping.update(value, AdminSettingsMapping.values(value).filterKeys { it != "sharedWifiNetworks" }, true)
            if (network == WirelessNetworkChoice.WIFI_DIRECT && CarlinkConnectionPolicy.usesWifiDirect(value)) {
                assertEquals(mapOf("networkMode" to if (value.carlinkOutput) CarlinkConnectionPolicy.REASON_KEY else CarlinkConnectionPolicy.MIRACAST_REASON_KEY), result.errors)
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
        assertEquals(before.copy(carBluetoothAudio = true), AdminSettingsMapping.update(before, mapOf("carBluetoothAudio" to true), true).settings)
        listOf(mapOf("unknown" to true), mapOf("autoStart" to "true"), mapOf("carBluetoothAudio" to "true"), mapOf("displayWidth" to 800.5),
            mapOf("output" to "invalid"), mapOf("iphone" to "bad address"), mapOf("carlinkName" to ""),
            mapOf("displayMode" to "custom", "displayWidth" to 801),
            mapOf("networkMode" to "shared-wifi", "sharedWifiNetworks" to listOf(mapOf("ssid" to ""))),
            mapOf("networkMode" to "phone-hotspot", "hotspotSsid" to "Phone", "hotspotPassword" to "short"))
            .forEach { val result = AdminSettingsMapping.update(before, it, true); assertNull(result.settings); assertTrue(result.errors.isNotEmpty()) }
        val updated = AdminSettingsMapping.update(before, mapOf("displayMode" to "custom", "displayWidth" to 1024,
            "displayHeight" to 600, "displayDensity" to 160, "output" to "carlink", "iphone" to "auto"), true).settings!!
        assertEquals(DisplayProfile(1024, 600, 160), updated.display)
        assertEquals(setOf(RelayOutput.CARLINK), updated.outputs)
        assertTrue(updated.carlinkOutput)
        assertNull(updated.iphoneAddress)
    }
    @Test fun outputChoicesKindsVisibilityAndPhoneOnlyCombinations() {
        val before = initial()
        val expected = listOf("web", "carlink", "wireless", "wireless")
        RelayOutput.entries.forEachIndexed { index, output ->
            val values = AdminSettingsMapping.values(before.copy(outputs = setOf(output)))
            assertEquals(expected[index], values["output"])
            assertEquals(if (output == RelayOutput.MIRACAST) "miracast" else "dlna", values["wirelessDisplay"])
        }
        val all = before.copy(outputs = RelayOutput.entries.toSet())
        assertEquals("multiple", AdminSettingsMapping.output(all))
        assertEquals(all, AdminSettingsMapping.update(all, mapOf("output" to "multiple"), true).settings)
        assertEquals(mapOf("output" to "error.outputsOnPhone"),
            AdminSettingsMapping.update(before, mapOf("output" to "multiple"), true).errors)
        val fields = AdminSettingsMapping.schema.flatMap { it.fields }
        assertEquals(listOf("wireless"), fields.single { it.key == "wirelessDisplay" }.visibleWhen!!.equals)
        assertEquals(listOf("carlink", "multiple"), fields.single { it.key == "carlinkName" }.visibleWhen!!.equals)
        for ((kind, output) in listOf("dlna" to RelayOutput.DLNA, "miracast" to RelayOutput.MIRACAST)) {
            val result = AdminSettingsMapping.update(before, mapOf("output" to "wireless", "wirelessDisplay" to kind), true)
            assertTrue(result.errors.toString(), result.errors.isEmpty())
            val chosen = result.settings!!
            assertEquals(setOf(output), chosen.outputs)
            assertEquals(if (kind == "miracast") WirelessNetworkChoice.PHONE_HOTSPOT else WirelessNetworkChoice.WIFI_DIRECT,
                chosen.wirelessNetwork.choice)
            if (kind == "miracast") assertEquals(mapOf("networkMode" to CarlinkConnectionPolicy.MIRACAST_REASON_KEY),
                AdminSettingsMapping.update(chosen, mapOf("networkMode" to "wifi-direct"), true).errors)
            val swapped = AdminSettingsMapping.update(chosen, mapOf("wirelessDisplay" to if (kind == "dlna") "miracast" else "dlna"), true)
            assertEquals(setOf(if (kind == "dlna") RelayOutput.MIRACAST else RelayOutput.DLNA), swapped.settings!!.outputs)
        }
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
