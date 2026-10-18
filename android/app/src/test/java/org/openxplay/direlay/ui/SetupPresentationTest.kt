// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-17
package org.openxplay.direlay.ui

import com.shilapi.xcertplay.orchestration.MfiTarget
import org.junit.Assert.*
import org.junit.Test
import org.openxplay.direlay.R
import org.openxplay.direlay.bluetooth.BondedDevice
import org.openxplay.direlay.bluetooth.IphoneSelection
import org.openxplay.direlay.settings.*

class SetupPresentationTest {
    private val settings = RelaySettings(ConnectionMode.WIRELESS, MfiTarget.LOCAL, "", "", "", null,
        30, false, true, false, null, wirelessNetwork = WirelessNetworkSettings(choice = WirelessNetworkChoice.WIFI_DIRECT))
    private val iphone = IphoneSelection.OnlyCandidate(BondedDevice("00:11:22:33:A9:30", "Phone", true, true, false))
    private val ready = HomeSnapshot(settings, wireless = true, permissionsReady = true, identityReady = true, iphoneSelection = iphone)

    @Test fun theNetworkStepExistsOnlyForWirelessCarPlay() {
        assertEquals(SetupStep.entries, setupSteps(wireless = true))
        assertEquals(listOf(SetupStep.OUTPUT, SetupStep.CONNECTION, SetupStep.PREPARE, SetupStep.START), setupSteps(wireless = false))
        assertEquals(SetupStep.NETWORK, setupStep("network"))
        assertNull(setupStep(""))
    }

    @Test fun recommendationsFollowTheOutput() {
        assertEquals(ConnectionMode.WIRELESS, recommendedConnection(settings))
        assertEquals(WirelessNetworkChoice.WIFI_DIRECT, recommendedNetwork(settings))
        val carlink = settings.copy(carlinkOutput = true)
        assertEquals(ConnectionMode.USB, recommendedConnection(carlink))
        assertEquals(WirelessNetworkChoice.PHONE_HOTSPOT, recommendedNetwork(carlink))
    }

    @Test fun choosingCarlinkLeavesWifiDirectAndReplacesBothOutputs() {
        val carlink = chooseOutput(settings, carlink = true, wireless = true)
        assertTrue(carlink.carlinkOutput)
        assertEquals(WirelessNetworkChoice.PHONE_HOTSPOT, carlink.wirelessNetwork.choice)
        val single = chooseOutput(settings.copy(bothOutputs = true), carlink = false, wireless = true)
        assertFalse(single.carlinkOutput || single.bothOutputs)
        // Over USB the network choice is unused, so it stays as it was.
        assertEquals(WirelessNetworkChoice.WIFI_DIRECT, chooseOutput(settings, carlink = true, wireless = false).wirelessNetwork.choice)
        val usbCarlink = settings.copy(carlinkOutput = true, connectionMode = ConnectionMode.USB)
        assertEquals(WirelessNetworkChoice.PHONE_HOTSPOT, chooseConnection(usbCarlink, wireless = true).wirelessNetwork.choice)
    }

    @Test fun leavingCarlinkReturnsAnUnsetHotspotToWifiDirect() {
        val carlink = chooseOutput(settings, carlink = true, wireless = true)
        assertEquals(WirelessNetworkChoice.PHONE_HOTSPOT, carlink.wirelessNetwork.choice)
        val browser = chooseOutput(carlink, carlink = false, wireless = true)
        assertEquals(WirelessNetworkChoice.WIFI_DIRECT, browser.wirelessNetwork.choice)
        assertTrue("the browser choice can be saved at once", settingsSaveErrors(browser, wireless = true).isEmpty())
        // A hotspot the user named stays chosen.
        val named = carlink.copy(wirelessNetwork = carlink.wirelessNetwork.copy(manualSsid = "Car"))
        assertEquals(WirelessNetworkChoice.PHONE_HOTSPOT, chooseOutput(named, carlink = false, wireless = true).wirelessNetwork.choice)
    }

    @Test fun networkFieldErrorsBlockOnlyTheNetworkPage() {
        val errors = mapOf("ssid" to R.string.settings_hotspot_name_error, "password" to R.string.settings_hotspot_password_error,
            "name" to R.string.settings_name_error)
        assertEquals(errors, setupStepErrors(SetupStep.NETWORK, errors))
        for (step in listOf(SetupStep.OUTPUT, SetupStep.CONNECTION)) {
            assertEquals(mapOf("name" to R.string.settings_name_error), setupStepErrors(step, errors))
        }
    }

    @Test fun theGuideEditsAnExplicitTransport() {
        val lastUsed = settings.copy(connectionMode = ConnectionMode.LAST_USED)
        assertEquals(ConnectionMode.WIRELESS, setupDraft(lastUsed, wireless = true).connectionMode)
        assertEquals(ConnectionMode.USB, setupDraft(lastUsed, wireless = false).connectionMode)
        assertSame(settings, setupDraft(settings, wireless = false))
    }

    @Test fun everyApplicableCheckIsListedWithItsState() {
        val home = presentHome(ready.copy(permissionsReady = false, identityReady = false, iphoneSelection = IphoneSelection.NoIphone))
        val checks = setupChecks(home, wireless = true, network = WirelessNetworkChoice.WIFI_DIRECT)
        assertEquals(listOf(R.string.setup_check_permissions, R.string.setup_check_identity, R.string.setup_check_iphone), checks.map { it.title })
        assertTrue(checks.all { it.state == SetupCheckState.TO_DO })
        assertEquals(SetupAction.IPHONE, checks.last().action)
        assertNull("the pairing row shows where to pair instead of repeating its title", checks.last().detail)
        assertEquals(R.string.setup_check_identity_hint, checks[1].detail)

        val done = setupChecks(presentHome(ready), wireless = true, network = WirelessNetworkChoice.WIFI_DIRECT)
        assertTrue(done.all { it.state == SetupCheckState.DONE && it.action == null })
    }

    @Test fun usbSkipsPairingAndSharedWifiAddsItsCheck() {
        val usb = setupChecks(presentHome(ready.copy(wireless = false)), wireless = false, network = WirelessNetworkChoice.WIFI_DIRECT)
        assertEquals(listOf(R.string.setup_check_permissions, R.string.setup_check_identity), usb.map { it.title })
        val shared = presentHome(ready.copy(sharedWifiReady = false, currentWifi = "Car",
            settings = settings.copy(wirelessNetwork = settings.wirelessNetwork.copy(choice = WirelessNetworkChoice.EXISTING_WIFI))))
        val check = setupChecks(shared, wireless = true, network = WirelessNetworkChoice.EXISTING_WIFI)
            .single { it.title == R.string.setup_check_shared_wifi }
        assertEquals(SetupAction.SHARED_WIFI, check.action)
        assertEquals(R.string.shared_wifi_not_saved, check.detail)
    }

    @Test fun theModeSummaryNamesEachChoiceAndTheStepThatChangesIt() {
        val wireless = modeRows(settings, wireless = true)
        assertEquals(listOf(SetupStep.OUTPUT, SetupStep.CONNECTION, SetupStep.NETWORK), wireless.map { it.step })
        assertEquals(listOf(R.string.ui_browser, R.string.direlay_wireless, R.string.direlay_wifi_direct), wireless.map { it.value })
        val usbCarlink = modeRows(settings.copy(carlinkOutput = true), wireless = false)
        assertEquals(listOf(R.string.ui_carlink, R.string.ui_usb), usbCarlink.map { it.value })
        assertEquals(R.string.dev_both, modeRows(settings.copy(bothOutputs = true), wireless = true).first().value)
    }

    @Test fun identityIsCheckingUntilTheStoreAnswers() {
        val checks = setupChecks(presentHome(ready.copy(identityReady = false, checking = true)), wireless = true,
            network = WirelessNetworkChoice.WIFI_DIRECT)
        assertEquals(SetupCheckState.CHECKING, checks.single { it.title == R.string.setup_check_identity }.state)
    }
    @Test fun hotspotFieldsAreRevealedForMissingCredentialsOrValidationErrors() {
        val hotspot = WirelessNetworkSettings(choice = WirelessNetworkChoice.PHONE_HOTSPOT,
            manualSsid = "Car", manualPassphrase = "password")
        assertFalse(revealHotspotFields(hotspot, emptyMap()))
        assertTrue(revealHotspotFields(hotspot.copy(manualSsid = ""), emptyMap()))
        assertTrue(revealHotspotFields(hotspot.copy(manualPassphrase = ""), emptyMap()))
        for (field in listOf("ssid", "password")) assertTrue(revealHotspotFields(hotspot, mapOf(field to 1)))
        for (choice in listOf(WirelessNetworkChoice.WIFI_DIRECT, WirelessNetworkChoice.EXISTING_WIFI))
            assertFalse(revealHotspotFields(hotspot.copy(choice = choice, manualSsid = ""), mapOf("ssid" to 1)))
    }

    @Test fun compactButtonsApplyOnlyBelow480Dp() {
        assertTrue(compactSetup(320)); assertTrue(compactSetup(479))
        assertFalse(compactSetup(480)); assertFalse(compactSetup(800))
    }

}
