// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-14
package org.openxplay.direlay.settings

import android.app.Application
import com.shilapi.xcertplay.AirPlayPersistence
import com.shilapi.xcertplay.orchestration.ManualHotspotBand
import com.shilapi.xcertplay.orchestration.ManualHotspotSecurity
import com.shilapi.xcertplay.orchestration.WirelessHotspotMode
import org.junit.Assert.*
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [28, 33], manifest = Config.NONE, application = Application::class)
class WirelessNetworkSettingsTest {
    private val context get() = RuntimeEnvironment.getApplication()
    private val store get() = RelaySettingsStore(context)
    private val configured = WirelessNetworkSettings(
        choice = WirelessNetworkChoice.PHONE_HOTSPOT,
        manualSsid = "Car hotspot", manualPassphrase = "car password",
        band = ManualHotspotBand.GHZ_5, channel = 36, security = ManualHotspotSecurity.WPA2,
        sharedWifiNetworks = listOf(SharedWifiNetwork("Home router", "router password")),
    )

    @Before fun clear() {
        for (name in listOf("direlay", "diplay", "xcertplay_airplay")) {
            context.getSharedPreferences(name, 0).edit().clear().commit()
        }
    }

    @Test fun choicesMapToDiPlayModesAndRoundTripThroughItsPersistence() {
        val mappings = mapOf(
            WirelessNetworkChoice.WIFI_DIRECT to WirelessHotspotMode.WIFI_P2P,
            WirelessNetworkChoice.PHONE_HOTSPOT to WirelessHotspotMode.MANUAL,
            WirelessNetworkChoice.EXISTING_WIFI to WirelessHotspotMode.EXISTING_WIFI,
        )
        for ((choice, mode) in mappings) {
            assertEquals(mode, choice.mode)
            assertEquals(choice, WirelessNetworkChoice.fromMode(mode))
            val network = configured.copy(choice = choice)
            store.save(store.load().copy(connectionMode = ConnectionMode.WIRELESS, wirelessNetwork = network))
            val storedMode = mode
            assertEquals(storedMode, AirPlayPersistence.loadWirelessHotspotMode(context))
            assertEquals(network.copy(choice = WirelessNetworkChoice.fromMode(storedMode)), store.load().wirelessNetwork)
            assertEquals("Car hotspot", AirPlayPersistence.loadManualHotspotSsid(context))
            assertEquals("car password", AirPlayPersistence.loadManualHotspotPassphrase(context))
            assertEquals(ManualHotspotBand.GHZ_5, AirPlayPersistence.loadManualHotspotBand(context))
            assertEquals(36, AirPlayPersistence.loadManualHotspotChannel(context))
            assertEquals(ManualHotspotSecurity.WPA2, AirPlayPersistence.loadManualHotspotSecurity(context))
            assertEquals(configured.sharedWifiNetworks, WirelessNetworkSettings.load(context).sharedWifiNetworks)
            assertFalse(context.getSharedPreferences("direlay", 0).all.keys.any {
                it.contains("ssid", true) || it.contains("passphrase", true) || it.contains("hotspot", true)
            })
        }
    }

    @Test fun changesFromDiPlayAndOpenNetworkCredentialsRemainVisible() {
        AirPlayPersistence.saveWirelessHotspotMode(context, WirelessHotspotMode.EXISTING_WIFI)
        AirPlayPersistence.saveExistingWifiCredentials(context, "Open router", "")
        AirPlayPersistence.saveManualHotspotSsid(context, "Other car")
        AirPlayPersistence.saveManualHotspotPassphrase(context, "other password")
        AirPlayPersistence.saveManualHotspotBand(context, ManualHotspotBand.GHZ_2_4)
        AirPlayPersistence.saveManualHotspotChannel(context, 11)
        AirPlayPersistence.saveManualHotspotSecurity(context, ManualHotspotSecurity.WPA3_TRANSITION)
        val network = store.load().wirelessNetwork
        assertEquals(WirelessNetworkChoice.EXISTING_WIFI, network.choice)
        assertEquals(listOf(SharedWifiNetwork("Open router", "")), network.sharedWifiNetworks)
        assertEquals("Other car", network.manualSsid)
        assertEquals("other password", network.manualPassphrase)
        assertEquals(ManualHotspotBand.GHZ_2_4, network.band)
        assertEquals(11, network.channel)
        assertEquals(ManualHotspotSecurity.WPA3_TRANSITION, network.security)
        store.save(store.load().copy(connectionMode = ConnectionMode.WIRELESS))
        assertEquals("", AirPlayPersistence.loadExistingWifiPassphrase(context))
    }

    @Test fun credentialsValidateUtf8ByteBoundariesAndPasswordLengths() {
        for (ssid in listOf("a", "a".repeat(32), "é".repeat(16), "車".repeat(10))) {
            for (password in listOf("", "p".repeat(8), "p".repeat(63))) {
                WirelessNetworkSettings.validateCredentials(ssid, password)
            }
        }
        for (ssid in listOf("", " ", "a".repeat(33), "é".repeat(17), "車".repeat(11), "bad\u0000ssid")) {
            assertTrue(runCatching { WirelessNetworkSettings.validateCredentials(ssid, "password") }.isFailure)
        }
        for (password in listOf("p", "p".repeat(7), "p".repeat(64), "bad\u0000password")) {
            assertTrue(runCatching { WirelessNetworkSettings.validateCredentials("Router", password) }.isFailure)
        }
    }

    @Test fun phoneHotspotValidatesBandChannelAndSecurity() {
        configured.validate()
        configured.copy(channel = 0).validate()
        configured.copy(security = ManualHotspotSecurity.OPEN, manualPassphrase = "").validate()
        configured.copy(band = ManualHotspotBand.GHZ_2_4, channel = 14).validate()
        for (network in listOf(
            configured.copy(channel = -1), configured.copy(channel = 197),
            configured.copy(band = ManualHotspotBand.GHZ_2_4, channel = 36),
            configured.copy(band = ManualHotspotBand.GHZ_5, channel = 11),
            configured.copy(manualPassphrase = ""), configured.copy(manualSsid = ""),
        )) assertTrue(runCatching { network.validate() }.isFailure)
    }

    @Test fun legacyLocalOnlyModeMapsToPhoneHotspot() {
        assertEquals(WirelessNetworkChoice.PHONE_HOTSPOT, WirelessNetworkChoice.fromMode(WirelessHotspotMode.LOCAL_ONLY_HOTSPOT))
    }

    @Test fun invalidWirelessInputsDoNotOverwriteSavedSettings() {
        store.save(store.load().copy(connectionMode = ConnectionMode.WIRELESS, wirelessNetwork = configured))
        val original = store.load()
        for (network in listOf(configured.copy(manualSsid = ""),
            configured.copy(choice = WirelessNetworkChoice.EXISTING_WIFI, sharedWifiNetworks = listOf(SharedWifiNetwork("Home router", "short"))))) {
            assertTrue(runCatching { store.save(original.copy(wirelessNetwork = network, autoStartOnBoot = true)) }.isFailure)
            assertEquals(original, store.load())
        }
    }

    @Test fun freshRelayStartsWithWifiDirect() {
        assertEquals(WirelessNetworkChoice.WIFI_DIRECT, store.load().wirelessNetwork.choice)
        assertEquals(WirelessHotspotMode.WIFI_P2P, AirPlayPersistence.loadWirelessHotspotMode(context))
        assertEquals(WirelessNetworkChoice.WIFI_DIRECT, store.load().wirelessNetwork.choice)
        assertFalse(context.getSharedPreferences("direlay", 0).getBoolean("wirelessNetworkChosen", false))
        // A later choice is kept rather than reset on the next load.
        store.save(store.load().copy(wirelessNetwork = configured))
        assertEquals(configured, store.load().wirelessNetwork)
    }

    @Test fun lastUsedWirelessValidatesCredentialsAndUsbIgnoresUnusedFields() {
        store.save(store.load().copy(connectionMode = ConnectionMode.USB))
        // Selecting a wireless mode with an unconfigured network is rejected.
        val unconfigured = WirelessNetworkSettings(choice = WirelessNetworkChoice.PHONE_HOTSPOT)
        assertTrue(runCatching {
            store.save(store.load().copy(connectionMode = ConnectionMode.WIRELESS, wirelessNetwork = unconfigured))
        }.isFailure)
        AirPlayPersistence.saveWirelessEnabled(context, true)
        assertTrue(runCatching {
            store.save(store.load().copy(connectionMode = ConnectionMode.LAST_USED, wirelessNetwork = unconfigured))
        }.isFailure)
        val direct = WirelessNetworkSettings(choice = WirelessNetworkChoice.WIFI_DIRECT)
        store.save(store.load().copy(connectionMode = ConnectionMode.WIRELESS, wirelessNetwork = direct))
        assertEquals(direct, store.load().wirelessNetwork)
    }

    @Test fun directLoadDefaultsToDirectUntilRelayRecordsAChoice() {
        assertEquals(WirelessNetworkChoice.WIFI_DIRECT, WirelessNetworkSettings.load(context).choice)
        assertFalse(context.getSharedPreferences("direlay", 0).getBoolean("wirelessNetworkChosen", false))
        configured.save(context)
        assertTrue(context.getSharedPreferences("direlay", 0).getBoolean("wirelessNetworkChosen", false))
        assertEquals(configured, WirelessNetworkSettings.load(context))
    }
    @Test fun upgradeKeepsEachExistingDiPlayChoice() {
        for (choice in WirelessNetworkChoice.entries) {
            clear()
            AirPlayPersistence.saveWirelessHotspotMode(context, choice.mode)
            assertEquals(choice, store.load().wirelessNetwork.choice)
            assertTrue(context.getSharedPreferences("direlay", 0).getBoolean("wirelessNetworkChosen", false))
        }
    }
    @Test fun sharedListPersistsPrivatelyAndLegacyCredentialsMigrateOnlyOnce() {
        AirPlayPersistence.saveExistingWifiCredentials(context, "Legacy", "legacy-password")
        assertEquals(listOf(SharedWifiNetwork("Legacy", "legacy-password")), WirelessNetworkSettings.load(context).sharedWifiNetworks)
        AirPlayPersistence.saveExistingWifiCredentials(context, "Other", "other-password")
        assertEquals("Legacy", WirelessNetworkSettings.load(context).sharedWifiNetworks.single().ssid)
        val list = listOf(SharedWifiNetwork("Home", "home-password"), SharedWifiNetwork("Car", ""))
        val settings = store.load().copy(connectionMode = ConnectionMode.WIRELESS,
            wirelessNetwork = WirelessNetworkSettings(choice = WirelessNetworkChoice.EXISTING_WIFI, sharedWifiNetworks = list))
        store.save(settings)
        assertEquals(list, store.load().wirelessNetwork.sharedWifiNetworks)
        assertEquals("Other", AirPlayPersistence.loadExistingWifiSsid(context))
        store.save(settings.copy(wirelessNetwork = settings.wirelessNetwork.copy(sharedWifiNetworks = emptyList())))
        assertTrue(store.load().wirelessNetwork.sharedWifiNetworks.isEmpty())
        assertTrue(runCatching { store.load().validate(true) }.isFailure)
        assertTrue(context.getSharedPreferences("direlay", 0).contains("sharedWifiNetworks"))
        assertEquals(emptyList<SharedWifiNetwork>(), WirelessNetworkSettings.load(context).sharedWifiNetworks)
    }
}
