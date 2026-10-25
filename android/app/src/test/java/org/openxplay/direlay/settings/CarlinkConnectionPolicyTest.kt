// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-16
package org.openxplay.direlay.settings

import android.app.Application
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config
import org.openxplay.direlay.admin.AdminSettingsMapping
import com.shilapi.xcertplay.AirPlayPersistence
import com.shilapi.xcertplay.orchestration.WirelessHotspotMode

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [28], manifest = Config.NONE, application = Application::class)
class CarlinkConnectionPolicyTest {
    private fun initial(): RelaySettings {
        val context = RuntimeEnvironment.getApplication()
        listOf("direlay", "diplay", "xcertplay_airplay").forEach { context.getSharedPreferences(it, 0).edit().clear().commit() }
        return RelaySettingsStore(context).use { it.load() }.copy(outputs = setOf(RelayOutput.BROWSER), connectionMode = ConnectionMode.WIRELESS, wirelessNetwork = WirelessNetworkSettings())
    }
    @Test fun miracastUsesWifiDirectWithoutChangingTheCarlinkEnabledMeaning() {
        val before = initial()
        for (carlink in listOf(false, true)) for (miracast in listOf(false, true))
            for (wireless in listOf(false, true)) for (p2p in listOf(false, true)) {
                assertEquals((carlink || miracast) && wireless && p2p,
                    CarlinkConnectionPolicy.conflicts(carlink, wireless, p2p, miracast = miracast))
            }
        for (choice in WirelessNetworkChoice.entries) {
            val candidate = before.copy(outputs = setOf(RelayOutput.BROWSER, RelayOutput.MIRACAST),
                wirelessNetwork = before.wirelessNetwork.copy(choice = choice, manualSsid = "Phone", manualPassphrase = "password"))
            val after = CarlinkConnectionPolicy.enabling(before, candidate, miracast = true)
            assertFalse(CarlinkConnectionPolicy.enabled(after))
            assertFalse(CarlinkConnectionPolicy.wifiDirectAllowed(after))
            assertTrue(CarlinkConnectionPolicy.wifiDirectAllowed(after, miracast = false))
            assertEquals(candidate.copy(wirelessNetwork = candidate.wirelessNetwork.copy(
                choice = if (choice == WirelessNetworkChoice.WIFI_DIRECT) WirelessNetworkChoice.PHONE_HOTSPOT else choice)), after)
            assertEquals(candidate, CarlinkConnectionPolicy.enabling(before, candidate, wireless = false, miracast = true))
        }
        val wired = before.copy(outputs = setOf(RelayOutput.BROWSER, RelayOutput.MIRACAST), connectionMode = ConnectionMode.USB)
        assertEquals(WirelessNetworkChoice.PHONE_HOTSPOT,
            CarlinkConnectionPolicy.enabling(wired, wired.copy(connectionMode = ConnectionMode.WIRELESS)).wirelessNetwork.choice)
        assertTrue(CarlinkConnectionPolicy.wifiDirectAllowed(before.copy(outputs = setOf(RelayOutput.BROWSER, RelayOutput.DLNA))))
    }
    @Test fun miracastStartConflictReadsResolvedTransportAndSettingsSaveAllowsTheHotspotSwitch() {
        val settings = initial().copy(outputs = setOf(RelayOutput.BROWSER, RelayOutput.MIRACAST))
        val context = RuntimeEnvironment.getApplication()
        AirPlayPersistence.saveWirelessHotspotMode(context, WirelessHotspotMode.WIFI_P2P)
        AirPlayPersistence.saveWirelessEnabled(context, true)
        RelaySettingsStore(context).use { store ->
            assertTrue(store.wifiDirectConflict(settings))
            assertFalse(store.carlinkWifiDirectConflict(settings))
            assertFalse(store.wifiDirectConflict(settings, miracast = false))
            assertFalse(store.wifiDirectConflict(settings.copy(connectionMode = ConnectionMode.USB)))
            assertTrue(store.wifiDirectConflict(settings.copy(connectionMode = ConnectionMode.LAST_USED)))
            AirPlayPersistence.saveWirelessEnabled(context, false)
            assertFalse(store.wifiDirectConflict(settings.copy(connectionMode = ConnectionMode.LAST_USED)))
            val switched = CarlinkConnectionPolicy.enabling(initial(), settings)
            assertTrue(org.openxplay.direlay.ui.settingsSaveErrors(switched, true).isEmpty())
            store.save(switched); assertEquals(switched, store.load())
            assertTrue(runCatching { switched.validate(true) }.isFailure)
        }
    }

    @Test fun onlyWirelessP2pConflicts() {
        for (carlink in listOf(false, true)) for (wireless in listOf(false, true)) for (p2p in listOf(false, true))
            assertEquals(carlink && wireless && p2p, CarlinkConnectionPolicy.conflicts(carlink, wireless, p2p))
    }
    @Test fun enablingEitherCarlinkOutputSwitchesOnlyWifiDirectAndPreservesCredentials() {
        val before = initial()
        for (both in listOf(false, true)) for (choice in WirelessNetworkChoice.entries) {
            val candidate = before.copy(outputs = OutputSelection.legacy(!both, both, false, false),
                wirelessNetwork = before.wirelessNetwork.copy(choice = choice, manualSsid = "Relay", manualPassphrase = "password"))
            val expected = candidate.copy(wirelessNetwork = candidate.wirelessNetwork.copy(
                choice = if (choice == WirelessNetworkChoice.WIFI_DIRECT) WirelessNetworkChoice.PHONE_HOTSPOT else choice))
            assertEquals(expected, CarlinkConnectionPolicy.enabling(before, candidate))
            assertFalse(CarlinkConnectionPolicy.wifiDirectAllowed(candidate))
            assertEquals(candidate, CarlinkConnectionPolicy.enabling(before, candidate, wireless = false))
        }
        assertTrue(CarlinkConnectionPolicy.wifiDirectAllowed(before))
    }
    @Test fun returningFromUsbWithCarlinkUsesHotspot() {
        val before = initial().copy(outputs = setOf(RelayOutput.CARLINK), connectionMode = ConnectionMode.USB)
        val after = CarlinkConnectionPolicy.enabling(before, before.copy(connectionMode = ConnectionMode.WIRELESS))
        assertEquals(WirelessNetworkChoice.PHONE_HOTSPOT, after.wirelessNetwork.choice)
        assertEquals(WirelessNetworkChoice.PHONE_HOTSPOT,
            AdminSettingsMapping.update(before, mapOf("transport" to "wireless"), false).settings!!.wirelessNetwork.choice)
    }
    @Test fun apiSwitchIsDurableWithoutHotspotSetupButStartValidationRemainsStrict() {
        val before = initial()
        for (output in listOf("carlink")) {
            val result = AdminSettingsMapping.update(before, mapOf("output" to output), true)
            assertTrue(result.errors.toString(), result.errors.isEmpty())
            val after = result.settings!!
            assertEquals(WirelessNetworkChoice.PHONE_HOTSPOT, after.wirelessNetwork.choice)
            val context = RuntimeEnvironment.getApplication()
            RelaySettingsStore(context).use { it.save(after); assertEquals(after, it.load()) }
            assertTrue(runCatching { after.validate(true) }.isFailure)
            assertEquals(mapOf("networkMode" to CarlinkConnectionPolicy.REASON_KEY),
                AdminSettingsMapping.update(after, mapOf("networkMode" to "wifi-direct"), true).errors)
            assertNotNull(AdminSettingsMapping.update(after, mapOf("transport" to "usb"), true).settings)
        }
    }
    @Test fun startAndCarlinkBlockReadTheTransportFromTheConnectionMode() {
        val settings = initial().copy(outputs = setOf(RelayOutput.CARLINK))
        val context = RuntimeEnvironment.getApplication()
        AirPlayPersistence.saveWirelessHotspotMode(context, WirelessHotspotMode.WIFI_P2P)
        // DiPlay's own flag still says wireless from an earlier session.
        AirPlayPersistence.saveWirelessEnabled(context, true)
        RelaySettingsStore(context).use { store ->
            assertFalse(store.carlinkWifiDirectConflict(settings.copy(connectionMode = ConnectionMode.USB)))
            assertTrue(store.carlinkWifiDirectConflict(settings.copy(connectionMode = ConnectionMode.WIRELESS)))
            assertTrue(store.carlinkWifiDirectConflict(settings.copy(connectionMode = ConnectionMode.LAST_USED)))
            assertFalse(store.carlinkWifiDirectConflict(settings.copy(outputs = setOf(RelayOutput.BROWSER))))
            AirPlayPersistence.saveWirelessEnabled(context, false)
            assertFalse(store.carlinkWifiDirectConflict(settings.copy(connectionMode = ConnectionMode.LAST_USED)))
            AirPlayPersistence.saveWirelessHotspotMode(context, WirelessHotspotMode.MANUAL)
            assertFalse(store.carlinkWifiDirectConflict(settings.copy(connectionMode = ConnectionMode.WIRELESS)))
        }
    }
}
