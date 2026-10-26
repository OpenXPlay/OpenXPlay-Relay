// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-14
package org.openxplay.direlay.ui

import org.junit.Assert.*
import org.junit.Test
import org.openxplay.direlay.R
import org.openxplay.direlay.relay.RelayFailure
import org.openxplay.direlay.path.*
import org.openxplay.direlay.vehicle.VehicleReport
import org.openxplay.direlay.vehicle.VehicleView

class HomePresentationTest {
    @Test fun vehicleSummaryUsesTheCurrentStateAndLatestRawValues() {
        val label: (Int, List<String>) -> String = { key, arguments -> when (key) {
            R.string.vehicle_off_value -> "Off"
            R.string.vehicle_waiting -> "Waiting for the car"
            R.string.vehicle_glance -> "Speed ${arguments[0]} · Gear ${arguments[1]} · Fuel ${arguments[2]}"
            else -> error("Unexpected resource $key")
        } }
        assertEquals("Off", vehicleSummary(VehicleView(), label))
        assertEquals("Waiting for the car", vehicleSummary(VehicleView(enabled = true), label))
        val reports = listOf(VehicleReport("gear", mapOf("speed" to "7", "gear" to "D"), 1),
            VehicleReport("gps", mapOf("speed" to "53"), 2), VehicleReport("fuel", mapOf("current" to "40"), 2))
        assertEquals("Speed 53 · Gear D · Fuel 40", vehicleSummary(VehicleView(enabled = true, reports = reports), label))
        assertEquals("Off", vehicleSummary(VehicleView(reports = reports), label))
        assertEquals("Speed — · Gear — · Fuel 40", vehicleSummary(VehicleView(enabled = true, reports = reports.takeLast(1)), label))
    }

    @Test fun recoveredCarlinkMessagesAreHiddenAndIdenticalDetailAndErrorAreMerged() {
        assertTrue(homeProblems(emptyList(), false, "auth_rejected", "previous error", null, "").isEmpty())
        assertEquals(listOf(HomeProblem(R.string.home_problem_carlink, "auth_rejected")),
            homeProblems(emptyList(), true, "auth_rejected", "auth_rejected", null, ""))
        assertEquals(listOf(HomeProblem(R.string.home_problem_carlink, "Socket closed")),
            homeProblems(emptyList(), true, "", "Socket closed", null, ""))
        assertEquals(listOf(HomeProblem(R.string.home_problem_carlink)),
            homeProblems(emptyList(), true, "", null, null, ""))
    }

    @Test fun carlinkRowCarriesExplanationRawTextTrailAndTime() {
        val trail = listOf("12:00:00.000 link_start", "+1.000s ble_write status=3")
        val error = "ble_write_failed\nCaused by: WRITE"
        val row = homeProblems(listOf(RelayFailure("clink", "duplicate")), true,
            "ble_write_failed", error, null, "", carlinkTrail = trail, carlinkErrorAt = 123L).single()
        assertEquals(error, row.text)
        assertEquals(R.string.error_explain_ble_write_failed, row.textResource)
        assertEquals("duplicate\n\n" + trail.joinToString("\n"), row.detail)
        assertEquals(123L, row.atMillis)
        assertEquals(R.string.error_explain_ble_write_failed,
            homeProblems(emptyList(), true, "", error, null, "").single().textResource)
        assertEquals(456L, homeProblems(listOf(RelayFailure(null, "Input: failed", 456L)),
            false, "", null, null, "").single().atMillis)
        assertNull(homeProblems(listOf(RelayFailure(null, "Input: failed")), false, "", null, null, "").single().atMillis)
    }

    @Test fun healthyCarlinkStatusTextIsNotShownAsTheFailure() {
        val row = homeProblems(listOf(RelayFailure("clink", "clink: Socket closed")), false,
            "Video passthrough", null, null, "").single()
        assertEquals("clink: Socket closed", row.text)
        assertNull(row.textResource)
        assertNull(row.detail)
    }

    @Test fun differentCarlinkErrorsAreAllKept() {
        val row = homeProblems(listOf(RelayFailure("clink", "clink: Socket closed")), true,
            "ble_service_missing", "ble_service_missing\nCaused by: SERVICE_MISSING", null, "",
            carlinkTrail = listOf("12:00:00.000 link_start")).single()
        assertEquals("ble_service_missing\nCaused by: SERVICE_MISSING", row.text)
        assertEquals(R.string.error_explain_ble_service_missing, row.textResource)
        assertEquals("clink: Socket closed\n\n12:00:00.000 link_start", row.detail)
    }

    @Test fun wifiDetailsIncludeTrailAfterABlankLine() {
        val row = homeProblems(emptyList(), true, "wifi_off", "wifi_off\nCaused by: WIFI_OFF", null, "",
            carlinkTrail = listOf("12:00:00.000 link_start"), carlinkErrorAt = 123L).single()
        assertEquals(R.string.network_wifi_problem, row.textResource)
        assertTrue(row.detail!!.endsWith("\n\n12:00:00.000 link_start"))
        assertEquals(123L, row.atMillis)
    }

    @Test fun knownBleFailuresHaveExplanationsAndOtherCodesDoNot() {
        val codes = listOf("ble_unavailable", "ble_timeout", "ble_scan_failed", "gatt_connect_failed",
            "gatt_services_failed", "ble_discovery_failed", "ble_service_missing", "ble_subscribe_failed",
            "ble_cccd_failed", "ble_cccd_timeout", "ble_write_failed", "ble_request_rejected", "ble_reply_invalid",
            "ble_verify_timeout", "ble_exchange_timeout", "ble_exchange_failed")
        codes.forEach { assertNotNull(it, carlinkFailureExplanation(it)) }
        assertEquals(carlinkFailureExplanation("gatt_services_failed"), carlinkFailureExplanation("ble_discovery_failed"))
        assertNull(carlinkFailureExplanation("unknown"))
        assertNull(carlinkFailureExplanation("ble_write_failed extra"))
    }

    @Test fun knownCarPlayStartupFailuresHaveLocalizedExplanations() {
        assertEquals(R.string.error_explain_hotspot_not_ready, carPlayFailureExplanation("hotspot_not_ready"))
        assertEquals(R.string.error_explain_hotspot_configuration, carPlayFailureExplanation("hotspot_configuration"))
        assertEquals(R.string.error_explain_first_tcp_timeout, carPlayFailureExplanation("first_tcp_timeout"))
        assertEquals(R.string.error_explain_bluetooth_unusable, carPlayFailureExplanation("bluetooth_unusable"))
        assertNull(carPlayFailureExplanation(null))
        assertNull(carPlayFailureExplanation("unknown"))
    }

    private val ready = PathInput(setup = SetupInput(network = NetworkPlan.PHONE_HOTSPOT),
        relay = RelayInput(identity = IdentityState.READY, bluetoothOn = true, hotspotOn = true),
        iphone = IphoneInput(pick = IphonePick.PICKED))

    @Test fun preparationUsesPathReadinessAndKeepsExistingSetupActions() {
        val input = ready.copy(setup = ready.setup.copy(valid = false),
            relay = ready.relay.copy(identity = IdentityState.MISSING, permissionsGranted = 0, permissionsTotal = 2),
            iphone = ready.iphone.copy(pick = IphonePick.NONE))
        val path = presentPath(input)
        val result = setupHomePresentation(input, path, emptyList())
        assertEquals(path.canStart, result.canStart)
        assertEquals(path.running, result.running)
        assertEquals(listOf(SetupAction.PERMISSIONS, SetupAction.IDENTITY, SetupAction.SETTINGS, SetupAction.IPHONE),
            result.checklist.map { it.action })
        assertFalse(result.checking)
        val checking = ready.copy(relay = ready.relay.copy(identity = IdentityState.CHECKING))
        assertTrue(setupHomePresentation(checking, presentPath(checking), emptyList()).checking)
        val running = ready.copy(iphone = ready.iphone.copy(armed = true))
        assertTrue(setupHomePresentation(running, presentPath(running), emptyList()).running)
        val shared = ready.copy(setup = ready.setup.copy(network = NetworkPlan.SHARED_WIFI),
            relay = ready.relay.copy(wifiSsid = "Car"), iphone = ready.iphone.copy(pick = IphonePick.NOT_IPHONE))
        assertEquals(listOf(SetupAction.SHARED_WIFI, SetupAction.CHOOSE_IPHONE),
            setupHomePresentation(shared, presentPath(shared), emptyList()).checklist.map { it.action })
        val usb = ready.copy(setup = ready.setup.copy(network = NetworkPlan.USB), iphone = IphoneInput())
        assertTrue(setupHomePresentation(usb, presentPath(usb), emptyList()).checklist.isEmpty())
    }

    @Test fun arrivalCountdownRequiresEveryEligibilityCondition() {
        for (mask in 0 until (1 shl 5)) {
            fun flag(bit: Int) = mask and (1 shl bit) != 0
            val path = ConnectionPath(canStart = flag(2), running = flag(3))
            assertEquals("mask=$mask", flag(0) && flag(1) && flag(2) && !flag(3) && !flag(4),
                arrivalCountdown(flag(0), flag(1), path, flag(4)))
        }
    }

    @Test fun onlyCountdownCompletionAndConnectAvoidPausingAutomaticStart() {
        ArrivalEnd.entries.forEach { reason ->
            assertEquals(reason.name, reason !in setOf(ArrivalEnd.COMPLETE, ArrivalEnd.CONNECT), reason.pauseAutomatic)
        }
        // Once consumed, a new ready snapshot cannot restart this arrival.
        assertFalse(arrivalCountdown(false, true, ConnectionPath(canStart = true), false))
    }

    @Test fun aStageStepResolvesByIdEvenWhenSeveralStepsShareItsAction() {
        val first = PathStep(StepId.BLUETOOTH, StepState.ACTION, R.string.design_step_bluetooth,
            action = StepAction.TURN_ON_BLUETOOTH)
        val second = first.copy(id = StepId.IPHONE_BLUETOOTH)
        val path = ConnectionPath(sections = listOf(PathSectionView(PathSection.RELAY, listOf(first)),
            PathSectionView(PathSection.IPHONE, listOf(second))))
        StepAction.entries.forEach { action ->
            val stage = StageAction.Step(action, second.id, null)
            assertEquals(StageCommand.Step(action, second), stageCommand(stage, path))
        }
        assertNull(stageCommand(StageAction.Step(StepAction.RETRY, StepId.CARLINK_LINK, null), path))
    }

    @Test fun animatedOutgoingActionsCannotToggleTheWrongSessionState() {
        val stopped = ConnectionPath(canStart = true)
        val running = stopped.copy(running = true)
        assertEquals(StageCommand.Connect, stageCommand(StageAction.Connect, stopped))
        assertNull(stageCommand(StageAction.Connect, running))
        assertEquals(StageCommand.Stop, stageCommand(StageAction.Stop, running))
        assertNull(stageCommand(StageAction.Stop, stopped))
        assertEquals(StageCommand.CancelArrival, stageCommand(StageAction.NotNow, stopped))
    }

    @Test fun stageNoticesResolveToTheExistingDestinations() {
        assertEquals(StageNoticeTarget.RESTART, stageNoticeTarget(StageNotice.DISPLAY_CHANGED))
        assertEquals(StageNoticeTarget.CAR_SETTINGS, stageNoticeTarget(StageNotice.CHOOSE_CAR))
        assertEquals(StageNoticeTarget.GUIDE, stageNoticeTarget(StageNotice.SET_UP))
        assertEquals("settings/car#car", relayRoute("settings:car"))
    }

    @Test fun idleWaitsCollapseWhileProblemsAndRunningWaitsExpand() {
        for (state in StepState.entries) {
            val section = PathSectionView(PathSection.CAR, listOf(PathStep(StepId.CAR_PAGE, state, R.string.path_car_page)))
            assertEquals(state in setOf(StepState.ACTION, StepState.FAILED, StepState.WORKING), expandPathSection(section, false))
            assertEquals(state in setOf(StepState.ACTION, StepState.FAILED, StepState.WORKING, StepState.WAITING), expandPathSection(section, true))
        }
        assertFalse(expandPathSection(PathSectionView(PathSection.CAR, emptyList()), true))
    }

    @Test fun sheetKeepsAllAddressesInOrderAndAddsOnlyRelevantFailureEvidence() {
        val step = presentPath(ready).steps.first { it.id == StepId.CAR_PAGE }
        val urls = listOf("https://relay.example/", "https://192.168.43.1/", "https://relay.example/")
        val details = stepDetails(step, urls, listOf(HomeProblem(R.string.home_problem_browser, "browser error", "trail"),
            HomeProblem(R.string.home_problem_carplay, "private iPhone detail")))
        assertEquals(listOf(urls[0], urls[1], "browser error", "trail"), details.trail)
        assertEquals(step.evidence, details.evidence)
        val identity = presentPath(ready).steps.first { it.id == StepId.IDENTITY }
        assertEquals(identity, stepDetails(identity, urls))
        assertEquals(listOf("old", "new"), stepDetails(identity.copy(trail = listOf("old", "new", "old")), urls).trail)
    }
    @Test fun startNowIsVisibleDuringPassiveCarWaitsButDoesNotHideProblems() {
        val input = ready.copy(car = CarInput(address = "192.168.43.1", web = WebServerInput(WebPhase.LISTENING, 9443)),
            iphone = ready.iphone.copy(started = true, carWaitUntilMillis = 30_000))
        val path = presentPath(input)
        assertEquals(StepId.CARPLAY_START, focusForPage(path)?.id)
        assertEquals(StepAction.START_NOW, focusForPage(path)?.action)
        val failed = path.steps.first { it.id == StepId.CAR_PAGE }.copy(state = StepState.FAILED)
        assertEquals(failed, focusForPage(path.copy(focus = failed)))
        val action = failed.copy(state = StepState.ACTION)
        assertEquals(action, focusForPage(path.copy(focus = action)))
        assertEquals(path.focus, focusForPage(path.copy(sections = emptyList())))
        assertNull(focusForPage(ConnectionPath()))
    }

}
