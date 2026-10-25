// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-17
package org.openxplay.direlay.ui

import org.openxplay.stream.core.FrameRatePreference
import com.shilapi.xcertplay.orchestration.MfiTarget
import org.junit.Assert.*
import org.junit.Test
import org.openxplay.direlay.R
import org.openxplay.direlay.bluetooth.BondedDevice
import org.openxplay.direlay.bluetooth.IphoneSelection
import org.openxplay.direlay.path.*
import org.openxplay.direlay.settings.*

class SetupPresentationTest {
    private val settings = RelaySettings(ConnectionMode.WIRELESS, MfiTarget.LOCAL, "", "", "", null,
        FrameRatePreference.AUTO, false, true, false, null, wirelessNetwork = WirelessNetworkSettings(choice = WirelessNetworkChoice.WIFI_DIRECT))
    private val ready = PathInput(relay = RelayInput(identity = IdentityState.READY), iphone = IphoneInput(pick = IphonePick.PICKED))
    private fun preparation(input: PathInput) = setupHomePresentation(input, presentPath(input), modeRows(settings, input.setup.wireless))

    @Test fun theNetworkStepExistsOnlyForWirelessCarPlay() {
        assertEquals(SetupStep.entries, setupSteps(wireless = true))
        assertEquals(listOf(SetupStep.CAR, SetupStep.OUTPUT, SetupStep.CONNECTION, SetupStep.PREPARE, SetupStep.START), setupSteps(wireless = false))
        assertEquals(SetupStep.CAR, setupSteps(wireless = true).first())
        assertEquals(SetupStep.CAR, setupStep("car"))
        assertEquals("setup:car", relayRoute("setup:car"))
        assertEquals(SetupStep.NETWORK, setupStep("network"))
        assertNull(setupStep(""))
    }

    @Test fun theCarStepBlocksNextUntilCompleteWithoutBlockingOtherSteps() {
        val incomplete = listOf(CarSelection(), CarSelection("xpeng"), CarSelection("other", model = "other"),
            CarSelection("other", "Brand", "other"), CarSelection("xpeng", model = "other", modelText = " "))
        for (car in incomplete) {
            assertTrue(setupStepErrors(SetupStep.CAR, emptyMap(), car).isNotEmpty())
            for (step in SetupStep.entries.filter { it != SetupStep.CAR })
                assertTrue(setupStepErrors(step, emptyMap(), car).isEmpty())
        }
        val complete = listOf(CarSelection("xpeng", model = "g6"), CarSelection("other", "Brand", "other", "Model"),
            CarSelection("xpeng", model = "other", modelText = "Model"), CarSelection("xpeng", model = "g6", year = "other"))
        for (car in complete) assertTrue(setupStepErrors(SetupStep.CAR, emptyMap(), car).isEmpty())
        assertTrue(setupStepErrors(SetupStep.CAR, mapOf("ssid" to R.string.settings_ssid_error), complete.first()).isEmpty())
    }

    @Test fun wirelessIsRecommendedWithOrWithoutCarlink() {
        for (mode in ConnectionMode.entries) for (carlink in listOf(false, true)) {
            assertEquals(ConnectionMode.WIRELESS, recommendedConnection(settings.copy(outputs = OutputSelection.legacy(carlink, false, false, false), connectionMode = mode)))
        }
        assertEquals(ConnectionMode.WIRELESS, recommendedConnection(settings.copy(outputs = setOf(RelayOutput.BROWSER, RelayOutput.CARLINK))))
    }

    @Test fun networkRecommendationsFollowTheOutput() {
        assertEquals(WirelessNetworkChoice.WIFI_DIRECT, recommendedNetwork(settings))
        val carlink = settings.copy(outputs = setOf(RelayOutput.CARLINK))
        assertEquals(WirelessNetworkChoice.PHONE_HOTSPOT, recommendedNetwork(carlink))
        assertEquals(WirelessNetworkChoice.WIFI_DIRECT, recommendedNetwork(settings.copy(outputs = setOf(RelayOutput.DLNA))))
        assertEquals(WirelessNetworkChoice.PHONE_HOTSPOT, recommendedNetwork(settings.copy(outputs = setOf(RelayOutput.MIRACAST))))
    }

    @Test fun onlySwitchingToUsbFromWirelessNeedsConfirmation() {
        assertTrue(needsUsbConfirmation(ConnectionMode.WIRELESS, ConnectionMode.USB, lastUsedWireless = false))
        assertFalse(needsUsbConfirmation(ConnectionMode.USB, ConnectionMode.USB, lastUsedWireless = true))
        assertFalse(needsUsbConfirmation(ConnectionMode.USB, ConnectionMode.WIRELESS, lastUsedWireless = false))
        assertFalse(needsUsbConfirmation(ConnectionMode.WIRELESS, ConnectionMode.WIRELESS, lastUsedWireless = true))
        for (mode in ConnectionMode.entries) for (lastUsedWireless in listOf(false, true))
            assertFalse(needsUsbConfirmation(mode, ConnectionMode.LAST_USED, lastUsedWireless))
    }

    @Test fun lastUsedNeedsUsbConfirmationOnlyWhenItResolvesToWireless() {
        assertTrue(needsUsbConfirmation(ConnectionMode.LAST_USED, ConnectionMode.USB, lastUsedWireless = true))
        assertFalse(needsUsbConfirmation(ConnectionMode.LAST_USED, ConnectionMode.USB, lastUsedWireless = false))
    }

    @Test fun choosingCarlinkLeavesWifiDirectAndReplacesBothOutputs() {
        val carlink = chooseOutput(settings, choice = OutputChoice.CARLINK, kind = WirelessDisplayKind.DLNA, wireless = true)
        assertTrue(carlink.carlinkOutput)
        assertEquals(WirelessNetworkChoice.PHONE_HOTSPOT, carlink.wirelessNetwork.choice)
        val single = chooseOutput(settings.copy(outputs = setOf(RelayOutput.BROWSER, RelayOutput.CARLINK)), choice = OutputChoice.BROWSER, kind = WirelessDisplayKind.DLNA, wireless = true)
        assertEquals(setOf(RelayOutput.BROWSER), single.outputs)
        // Over USB the network choice is unused, so it stays as it was.
        assertEquals(WirelessNetworkChoice.WIFI_DIRECT, chooseOutput(settings, choice = OutputChoice.CARLINK, kind = WirelessDisplayKind.DLNA, wireless = false).wirelessNetwork.choice)
        val usbCarlink = settings.copy(outputs = setOf(RelayOutput.CARLINK), connectionMode = ConnectionMode.USB)
        assertEquals(WirelessNetworkChoice.PHONE_HOTSPOT, chooseConnection(usbCarlink, wireless = true).wirelessNetwork.choice)
    }

    @Test fun leavingCarlinkReturnsAnUnsetHotspotToWifiDirect() {
        val carlink = chooseOutput(settings, choice = OutputChoice.CARLINK, kind = WirelessDisplayKind.DLNA, wireless = true)
        assertEquals(WirelessNetworkChoice.PHONE_HOTSPOT, carlink.wirelessNetwork.choice)
        val browser = chooseOutput(carlink, choice = OutputChoice.BROWSER, kind = WirelessDisplayKind.DLNA, wireless = true)
        assertEquals(WirelessNetworkChoice.WIFI_DIRECT, browser.wirelessNetwork.choice)
        assertTrue("the browser choice can be saved at once", settingsSaveErrors(browser, wireless = true).isEmpty())
        // A hotspot the user named stays chosen.
        val named = carlink.copy(wirelessNetwork = carlink.wirelessNetwork.copy(manualSsid = "Car"))
        assertEquals(WirelessNetworkChoice.PHONE_HOTSPOT, chooseOutput(named, choice = OutputChoice.BROWSER, kind = WirelessDisplayKind.DLNA, wireless = true).wirelessNetwork.choice)
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
        val home = preparation(ready.copy(relay = ready.relay.copy(identity = IdentityState.MISSING, permissionsTotal = 1), iphone = IphoneInput()))
        val checks = setupChecks(home, wireless = true, network = WirelessNetworkChoice.WIFI_DIRECT)
        assertEquals(listOf(R.string.setup_check_permissions, R.string.setup_check_identity, R.string.setup_check_iphone), checks.map { it.title })
        assertTrue(checks.all { it.state == SetupCheckState.TO_DO })
        assertEquals(SetupAction.IPHONE, checks.last().action)
        assertNull("the pairing row shows where to pair instead of repeating its title", checks.last().detail)
        assertEquals(R.string.setup_check_identity_hint, checks[1].detail)

        val done = setupChecks(preparation(ready), wireless = true, network = WirelessNetworkChoice.WIFI_DIRECT)
        assertTrue(done.all { it.state == SetupCheckState.DONE && it.action == null })
    }

    @Test fun usbSkipsPairingAndSharedWifiAddsItsCheck() {
        val usb = setupChecks(preparation(ready.copy(setup = ready.setup.copy(network = NetworkPlan.USB))), wireless = false, network = WirelessNetworkChoice.WIFI_DIRECT)
        assertEquals(listOf(R.string.setup_check_permissions, R.string.setup_check_identity), usb.map { it.title })
        val shared = preparation(ready.copy(setup = ready.setup.copy(network = NetworkPlan.SHARED_WIFI), relay = ready.relay.copy(wifiSsid = "Car")))
        val check = setupChecks(shared, wireless = true, network = WirelessNetworkChoice.EXISTING_WIFI)
            .single { it.title == R.string.setup_check_shared_wifi }
        assertEquals(SetupAction.SHARED_WIFI, check.action)
        assertEquals(R.string.shared_wifi_not_saved, check.detail)
    }

    @Test fun theModeSummaryNamesEachChoiceAndTheStepThatChangesIt() {
        val wireless = modeRows(settings, wireless = true)
        assertEquals(listOf(SetupStep.OUTPUT, SetupStep.CONNECTION, SetupStep.NETWORK), wireless.map { it.step })
        assertEquals(listOf(R.string.ui_browser, R.string.direlay_wireless, R.string.direlay_wifi_direct), wireless.map { it.value })
        val usbCarlink = modeRows(settings.copy(outputs = setOf(RelayOutput.CARLINK)), wireless = false)
        assertEquals(listOf(R.string.ui_carlink, R.string.ui_usb), usbCarlink.map { it.value })
        assertEquals(R.string.output_multiple, modeRows(settings.copy(outputs = setOf(RelayOutput.BROWSER, RelayOutput.CARLINK)), wireless = true).first().value)
    }

    @Test fun wirelessAndCombinationModeRowsAndAfterStartLinesUseEnumOrder() {
        val labels = listOf(R.string.ui_browser, R.string.ui_carlink, R.string.wireless_dlna, R.string.wireless_miracast)
        val icons = listOf(R.drawable.symbol_monitor, R.drawable.symbol_car_front, R.drawable.symbol_cast, R.drawable.symbol_cast)
        val lines = listOf(R.string.setup_after_browser, R.string.setup_after_carlink, R.string.setup_after_dlna, R.string.setup_after_miracast)
        RelayOutput.entries.forEachIndexed { index, output ->
            val chosen = settings.copy(outputs = setOf(output))
            assertEquals(labels[index], modeRows(chosen, true).first().value)
            assertEquals(icons[index], modeRows(chosen, true).first().icon)
            assertEquals(listOf(R.string.setup_after_wireless, lines[index]), afterStartLines(chosen, true))
            assertEquals(listOf(R.string.setup_after_usb, lines[index]), afterStartLines(chosen, false))
        }
        val combined = settings.copy(outputs = RelayOutput.entries.reversed().toSet())
        assertEquals(R.string.output_multiple, modeRows(combined, true).first().value)
        assertEquals(R.drawable.symbol_link, modeRows(combined, true).first().icon)
        assertEquals(listOf(R.string.setup_after_wireless) + lines, afterStartLines(combined, true))
    }

    @Test fun overlayConsentIsIncludedInPreparationChecks() {
        val input = ready.copy(relay = ready.relay.copy(overlayNeeded = true))
        val checks = setupChecks(preparation(input), true, WirelessNetworkChoice.WIFI_DIRECT)
        val permissions = checks.single { it.title == R.string.setup_check_permissions }
        assertEquals(SetupCheckState.TO_DO, permissions.state)
        assertEquals(SetupAction.PERMISSIONS, permissions.action)
        assertFalse(preparation(input).canStart)
    }

    @Test fun identityIsCheckingUntilTheStoreAnswers() {
        val checks = setupChecks(preparation(ready.copy(relay = ready.relay.copy(identity = IdentityState.CHECKING))), wireless = true,
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

    @Test fun preparationKeepsEveryRelayStepAndOnlyThePairingPrerequisite() {
        val path = presentPath(PathInput(relay = RelayInput(keepAlive = KeepAliveState.REFUSED)))
        val sections = setupPrepareSections(path)
        assertEquals(listOf(PathSection.RELAY, PathSection.IPHONE), sections.map { it.section })
        assertSame(path.section(PathSection.RELAY), sections.first())
        val paired = path.steps.single { it.id == StepId.IPHONE_PAIRED }
        assertEquals(listOf(paired), sections.last().steps)
        assertEquals(StepAction.PAIR_IPHONE, sections.last().steps.single().action)
    }

    @Test fun usbPreparationKeepsItsLiveRoleAndFailure() {
        for (role in UsbRole.entries) {
            val path = presentPath(PathInput(setup = SetupInput(network = NetworkPlan.USB), iphone = IphoneInput(usb = role)))
            val sections = setupPrepareSections(path)
            assertEquals(listOf(PathSection.RELAY, PathSection.IPHONE), sections.map { it.section })
            assertEquals(listOf(path.steps.single { it.id == StepId.USB_HOST }), sections.last().steps)
        }
    }

    @Test fun preparationOmitsEmptyOrMissingSections() {
        assertTrue(setupPrepareSections(ConnectionPath()).isEmpty())
        val path = ConnectionPath(sections = listOf(PathSectionView(PathSection.RELAY, emptyList()),
            PathSectionView(PathSection.IPHONE, listOf(PathStep(StepId.SESSION, StepState.WORKING, R.string.path_session)))))
        assertTrue(setupPrepareSections(path).isEmpty())
        val relay = PathSectionView(PathSection.RELAY, listOf(PathStep(StepId.MODE, StepState.OK, R.string.path_mode)))
        assertEquals(listOf(relay), setupPrepareSections(ConnectionPath(sections = listOf(relay))))
    }

}
