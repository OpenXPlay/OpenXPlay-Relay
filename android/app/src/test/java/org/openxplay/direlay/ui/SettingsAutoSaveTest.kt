// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-15
package org.openxplay.direlay.ui

import com.shilapi.xcertplay.orchestration.MfiTarget
import org.junit.Assert.*
import org.junit.Test
import org.openxplay.direlay.settings.*
import org.openxplay.direlay.R

class SettingsAutoSaveTest {
    private val initial = RelaySettings(ConnectionMode.WIRELESS, MfiTarget.LOCAL, "", "", "", null,
        30, false, true, false, null, wirelessNetwork = WirelessNetworkSettings(choice = WirelessNetworkChoice.WIFI_DIRECT))
    @Test fun teslaEditsApplyOnNextStartAndInvalidAddressesBelongToTheCarPage() {
        for (chosen in listOf(initial.copy(teslaBrowser = TeslaBrowserMode.PUBLIC_ADDRESS),
            initial.copy(teslaBrowser = TeslaBrowserMode.VIRTUAL_NAME), initial.copy(teslaVirtualAddress = "100.88.0.2"),
            initial.copy(teslaPublicAddress = "3.3.3.2"))) {
            assertEquals(SettingsNotice.NEXT_START, settingsNotice(initial, chosen, false))
            assertEquals(SettingsNotice.NEXT_START, settingsNotice(initial, chosen, true))
        }
        val invalid = initial.copy(teslaBrowser = TeslaBrowserMode.PUBLIC_ADDRESS, teslaPublicAddress = "10.0.0.1")
        assertEquals(mapOf("teslaAddress" to R.string.tesla_address_invalid), settingsSaveErrors(invalid, true))
        assertEquals(setOf("settings/car"), settingsInvalidPages(invalid, initial) { true })
        val published = mutableListOf<RelaySettings>()
        val saver = SettingsAutoSave(initial, published::add)
        assertFalse(saver.apply(invalid, true))
        assertTrue(saver.apply(invalid.copy(teslaBrowser = TeslaBrowserMode.OFF), true))
    }
    @Test fun validChangesPublishImmediatelyAndIdenticalDraftsDoNotPublish() {
        val published = mutableListOf<RelaySettings>()
        val saver = SettingsAutoSave(initial, published::add)
        val updated = initial.copy(carlinkOutput = true, vehicleInformation = true)
        assertTrue(saver.apply(updated, true)); assertEquals(updated, saver.saved)
        assertFalse(saver.apply(updated, true)); assertEquals(listOf(updated), published)
        assertFalse(initial.vehicleInformation)
    }
    @Test fun editsOnTwoPagesShareTheDraftAndPublishEachValidChangeOnlyOnce() {
        val published = mutableListOf<RelaySettings>()
        val saver = SettingsAutoSave(initial, published::add)
        var stack = RelayBackStack().open("settings:output")
        var draft = initial.copy(carlinkDeviceName = "")
        assertFalse(saver.apply(draft, true))
        stack = stack.open("settings/startup")
        draft = draft.copy(autoStartOnBoot = true)
        assertFalse(saver.apply(draft, true))
        stack = stack.open("settings:output")
        assertEquals("settings/output#output", stack.current)
        assertTrue(draft.autoStartOnBoot)
        draft = draft.copy(carlinkDeviceName = "My relay")
        assertTrue(saver.apply(draft, true))
        assertFalse(saver.apply(draft, true))
        stack = stack.open("settings/picture")
        draft = draft.copy(videoCodec = VideoCodecSetting.entries.last())
        assertTrue(saver.apply(draft, true))
        assertFalse(saver.apply(draft, true))
        assertEquals("settings/picture", stack.current)
        assertEquals(listOf(initial.copy(carlinkDeviceName = "My relay", autoStartOnBoot = true), draft), published)
    }

    @Test fun menuErrorsIdentifyEveryInvalidPageEvenWhenAnotherGroupIsInvalid() {
        val invalid = initial.copy(carlinkDeviceName = "", carlinkDeviceModel = "", webPort = 80,
            display = DisplayProfile(1, 2, 3), mfiTarget = MfiTarget.REMOTE, remoteServer = "invalid",
            wirelessNetwork = initial.wirelessNetwork.copy(choice = WirelessNetworkChoice.PHONE_HOTSPOT,
                manualSsid = "", manualPassphrase = "short"))
        assertEquals(setOf("settings/output", "settings/connection/network", "developer/video",
            "developer/connection", "settings/identity"), settingsInvalidPages(invalid, initial) { true })
        assertTrue(settingsInvalidPages(initial, initial) { true }.isEmpty())
    }

    @Test fun identityValidationMarksOnlyTheTopLevelIdentityPage() {
        for (invalid in listOf(initial.copy(mfiTarget = MfiTarget.REMOTE, remoteServer = "invalid"),
            initial.copy(mfiTarget = MfiTarget.I2C, i2cPath = ""), initial.copy(remoteToken = "\u0000"))) {
            val pages = settingsInvalidPages(invalid, initial) { true }
            assertEquals(setOf("settings/identity"), pages)
            assertFalse(pages.any { it.startsWith("developer/") })
        }
    }

    @Test fun invalidCredentialsStayDraftUntilValidAndIrrelevantCredentialsDoNotBlockUsb() {
        val published = mutableListOf<RelaySettings>()
        val saver = SettingsAutoSave(initial, published::add)
        val invalid = initial.copy(wirelessNetwork = initial.wirelessNetwork.copy(choice = WirelessNetworkChoice.PHONE_HOTSPOT, manualSsid = "", manualPassphrase = "short"))
        assertEquals(setOf("ssid", "password"), settingsErrors(invalid, true).keys)
        assertFalse(saver.apply(invalid, true)); assertEquals(initial, saver.saved)
        val valid = invalid.copy(wirelessNetwork = invalid.wirelessNetwork.copy(manualSsid = "Car", manualPassphrase = "12345678"))
        assertTrue(saver.apply(valid, true))
        assertTrue(saver.apply(invalid.copy(connectionMode = ConnectionMode.USB), false))
        assertEquals(2, published.size)
    }
    @Test fun failedPublicationDoesNotAdvanceSavedState() {
        val saver = SettingsAutoSave(initial) { error("disk") }
        try { saver.apply(initial.copy(autoStartOnBoot = true), true); fail("Expected failure") } catch (_: IllegalStateException) { }
        assertEquals(initial, saver.saved)
        assertTrue(settingsErrors(initial.copy(carlinkDeviceName = ""), true).containsKey("name"))
        assertTrue(settingsErrors(initial.copy(webPort = 80), true).containsKey("advanced"))
    }

    @Test fun ordinaryAutoSavesAreSilentAndSessionChangesExplainTheirTiming() {
        assertEquals(SettingsNotice.SILENT, settingsNotice(initial, initial, true))
        for (value in listOf(initial.copy(autoStartWhenReady = false), initial.copy(autoStartOnBoot = true),
            initial.copy(localPreview = true), initial.copy(playCarPlayAudio = true), initial.copy(webPort = 9443),
            initial.copy(carlinkDeviceName = "Car"), initial.copy(carlinkOutput = true))) {
            assertEquals(SettingsNotice.SILENT, settingsNotice(initial, value, false))
        }
        for (connected in listOf(false, true)) {
            assertEquals(SettingsNotice.SILENT, settingsNotice(initial, initial.copy(carlinkReconnectOnResize = true), connected))
        }
        for (value in listOf(initial.copy(connectionMode = ConnectionMode.USB), initial.copy(fps = 60),
            initial.copy(display = DisplayProfile(800, 480, 160)), initial.copy(mfiTarget = MfiTarget.USB_CH341),
            initial.copy(i2cPath = "/dev/i2c-2"), initial.copy(remoteServer = "https://example.test"),
            initial.copy(remoteToken = "new"), initial.copy(carBluetoothAudio = false), initial.copy(wirelessNetwork = initial.wirelessNetwork.copy(sharedWifiNetworks = listOf(SharedWifiNetwork("Car", "")))))) {
            assertEquals(SettingsNotice.NEXT_START, settingsNotice(initial, value, false))
            assertEquals(SettingsNotice.NEXT_START, settingsNotice(initial, value, true))
        }
        assertEquals(SettingsNotice.RECONNECT, settingsNotice(initial, initial.copy(vehicleInformation = true), true))
        assertEquals(SettingsNotice.SILENT, settingsNotice(initial, initial.copy(vehicleInformation = true), false))
    }
    @Test fun radioRowsHaveDistinctTitlesAndSupportingCopyForEveryChoice() {
        assertEquals(listOf(RadioOption(R.string.ui_browser, R.string.option_browser_hint),
            RadioOption(R.string.ui_carlink, R.string.option_carlink_hint)), outputOptions)
        assertEquals(listOf(R.string.option_wireless_hint, R.string.option_usb_hint), connectionOptions.map { it.supporting })
        val network = WirelessNetworkChoice.entries.map(::networkOption)
        assertEquals(listOf(R.string.ui_wifi_direct_recommended, R.string.direlay_car_hotspot,
            R.string.direlay_existing_wifi), network.map { it.title })
        assertEquals(listOf(R.string.option_direct_hint, R.string.option_hotspot_hint,
            R.string.option_wifi_hint), network.map { it.supporting })
        assertEquals(7, (outputOptions + connectionOptions + network).map { it.supporting }.distinct().size)
    }

    @Test fun onlyDeferredChangesHaveNoticesAndRunningSessionsOfferAnAction() {
        assertNull(settingsFeedback(initial, initial.copy(autoStartOnBoot = true), false, true))
        val next = initial.copy(fps = 60)
        assertEquals(SettingsFeedback(R.string.settings_next_start), settingsFeedback(initial, next, false, false))
        assertEquals(SettingsFeedback(R.string.settings_next_start, R.string.settings_restart), settingsFeedback(initial, next, false, true))
        assertEquals(SettingsFeedback(R.string.vehicle_reconnect, R.string.vehicle_reconnect_action),
            settingsFeedback(initial, initial.copy(vehicleInformation = true), true, true))
    }

    @Test fun diagramsDescribeEachTopologyAndValidationUsesTheSelectedNetwork() {
        assertEquals(listOf(R.string.network_direct_description, R.string.network_hotspot_description,
            R.string.network_wifi_description), WirelessNetworkChoice.entries.map(::networkDescription))
        val hotspot = initial.copy(wirelessNetwork = WirelessNetworkSettings(choice = WirelessNetworkChoice.PHONE_HOTSPOT,
            manualPassphrase = "short", sharedWifiNetworks = listOf(SharedWifiNetwork("Valid router", "password"))))
        assertEquals(mapOf("ssid" to R.string.settings_hotspot_name_error,
            "password" to R.string.settings_hotspot_password_error), settingsErrors(hotspot, true))
        assertTrue(settingsErrors(hotspot.copy(wirelessNetwork = hotspot.wirelessNetwork.copy(choice = WirelessNetworkChoice.EXISTING_WIFI)), true).isEmpty())
        val wifi = initial.copy(wirelessNetwork = WirelessNetworkSettings(choice = WirelessNetworkChoice.EXISTING_WIFI,
            manualSsid = "Valid phone", manualPassphrase = "password", sharedWifiNetworks = emptyList()))
        assertEquals(mapOf("ssid" to R.string.shared_wifi_empty), settingsErrors(wifi, true))
        assertTrue(settingsErrors(wifi.copy(wirelessNetwork = wifi.wirelessNetwork.copy(choice = WirelessNetworkChoice.WIFI_DIRECT)), true).isEmpty())
    }
}
