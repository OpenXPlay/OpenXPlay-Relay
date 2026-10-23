// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-14
package org.openxplay.direlay.ui

import com.shilapi.xcertplay.orchestration.MfiTarget
import org.junit.Assert.*
import org.junit.Test
import org.openxplay.direlay.bluetooth.IphoneSelection
import org.openxplay.direlay.bluetooth.BondedDevice
import org.openxplay.direlay.R
import org.openxplay.direlay.relay.WebStartReason
import org.openxplay.direlay.relay.WebStartFailure
import org.openxplay.direlay.relay.RelayFailure
import org.openxplay.direlay.relay.CarPlayStage
import org.openxplay.direlay.settings.*
import org.openxplay.stream.core.SessionPhase
import org.openxplay.direlay.vehicle.VehicleReport
import org.openxplay.direlay.vehicle.VehicleView

class HomePresentationTest {
    private val settings = RelaySettings(ConnectionMode.WIRELESS, MfiTarget.LOCAL, "", "", "", null,
        30, false, true, false, null, wirelessNetwork = WirelessNetworkSettings(choice = WirelessNetworkChoice.WIFI_DIRECT))
    private val resolved = IphoneSelection.OnlyCandidate(BondedDevice("00:11:22:33:A9:30", "Phone", true, true, false))
    private val ready = HomeSnapshot(settings, wireless = true, permissionsReady = true, identityReady = true, iphoneSelection = resolved)

    @Test fun connectionSummaryShowsTransportOutputAndOnlyTheEnabledCarlinkState() {
        val labels = mapOf(R.string.direlay_wireless to "Wireless", R.string.ui_usb to "USB",
            R.string.ui_browser to "Browser", R.string.ui_carlink to "Carlink", R.string.dev_both to "Browser + Carlink",
            R.string.ui_connected to "Connected", R.string.ui_searching to "Searching")
        for (wireless in listOf(false, true)) for (carlink in listOf(false, true)) for (both in listOf(false, true)) {
            val selected = ready.copy(wireless = wireless, settings = settings.copy(carlinkOutput = carlink, bothOutputs = both))
            val transport = if (wireless) "Wireless" else "USB"
            val output = if (both) "Browser + Carlink" else if (carlink) "Carlink" else "Browser"
            for (state in listOf(R.string.ui_connected, R.string.ui_searching)) {
                val result = connectionSummary(presentHome(selected).mode, state.takeIf { carlink || both }) { labels.getValue(it) }
                assertEquals("$transport · $output" + if (carlink || both) " · ${labels.getValue(state)}" else "", result)
                assertFalse(result.contains('\n'))
            }
        }
        val baseline = connectionSummary(presentHome(ready).mode, null) { labels.getValue(it) }
        for (network in WirelessNetworkChoice.entries) assertEquals(baseline,
            connectionSummary(presentHome(ready.copy(settings = settings.copy(
                wirelessNetwork = settings.wirelessNetwork.copy(choice = network)))).mode, null) { labels.getValue(it) })
    }

    @Test fun networkSummaryUsesOnlyTheFirstNonblankAddressAndHandlesMissingValues() {
        assertEquals("Car Wi-Fi · 192.168.1.2", networkSummary("Car Wi-Fi", "\n192.168.1.2\n10.0.0.2"))
        assertEquals("Car Wi-Fi", networkSummary(" Car Wi-Fi ", " \n"))
        assertEquals("192.168.1.2", networkSummary("", "192.168.1.2\n10.0.0.2"))
        assertEquals("No network", networkSummary("No network", "No network"))
        assertEquals("", networkSummary("", ""))
    }

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

    @Test fun carlinkStateLabelsMatchTheHomeStatesForTheRunningPage() {
        for (state in listOf("ACTIVE", "CONNECTED", "STREAMING", "PLAYING"))
            assertEquals(R.string.ui_connected, carlinkStateLabel(state, false, false))
        assertEquals(R.string.ui_waiting_code, carlinkStateLabel("ACTIVE", false, true))
        assertEquals(R.string.ui_carlink_unsupported, carlinkStateLabel("Requires Android 10+", true, false))
        assertEquals(R.string.ui_carlink_failed, carlinkStateLabel("ENDED", false, false))
        assertEquals(R.string.ui_carlink_failed, carlinkStateLabel("SEARCHING", true, false))
        assertEquals(R.string.ui_searching, carlinkStateLabel("SEARCHING", false, false))
    }

    @Test fun errorCardCarriesEveryProblemSourceWithMessagesAndCodesIntact() {
        val reason = WebStartReason(WebStartFailure.PORT_BUSY, 9443)
        val listenerError = "Carlink session failed\nCaused by: auth_rejected"
        val webError = "Web receiver unavailable: Cannot bind\nCaused by: Address already in use"
        val problems = homeProblems(listOf(
            RelayFailure(null, "Input: CarPlay start failed\nCaused by: connection reset"),
            RelayFailure("web", "web: Video output closed"),
            RelayFailure("clink", "clink: Socket closed"),
            RelayFailure("other-output", "other-output: Output failed")),
            true, "auth_rejected", listenerError, reason, webError)
        val result = presentHome(ready.copy(failure = true, phase = SessionPhase.ACTIVE,
            settings = settings.copy(bothOutputs = true), webReason = reason, problems = problems))
        assertEquals(StatusTone.ERROR, result.tone)
        assertEquals(listOf(
            HomeProblem(R.string.home_problem_carplay, "Input: CarPlay start failed\nCaused by: connection reset"),
            HomeProblem(R.string.home_problem_browser, "web: Video output closed"),
            HomeProblem(R.string.home_problem_output, "other-output: Output failed"),
            HomeProblem(R.string.home_problem_carlink, "auth_rejected\n$listenerError", "clink: Socket closed"),
            HomeProblem(R.string.home_problem_web_server, webError, reason.toString())), result.problems)
        assertEquals(R.string.ui_web_port_busy, result.subline)
        assertEquals(listOf("9443"), result.sublineArguments)
    }

    @Test fun carlinkPolicyAndVideoFailuresAndOtherWebStartFailuresHaveRows() {
        for (detail in listOf(CarlinkConnectionPolicy.REASON, "Head unit cannot decode this video",
            "Head unit cannot decode this video\npipeline_failed\nCodec failed\nCaused by: hardware_codec_unavailable")) {
            val problems = homeProblems(emptyList(), true, detail, null, null, "")
            assertEquals(listOf(HomeProblem(R.string.home_problem_carlink, detail)),
                presentHome(ready.copy(failure = true, problems = problems)).problems)
        }
        val reason = WebStartReason(WebStartFailure.OTHER, 8443)
        val problems = homeProblems(emptyList(), false, "", null, reason, "Web receiver unavailable: SecurityException")
        val result = presentHome(ready.copy(webReason = reason, problems = problems))
        assertEquals(listOf(HomeProblem(R.string.home_problem_web_server,
            "Web receiver unavailable: SecurityException", reason.toString())), result.problems)
        assertEquals(R.string.ui_web_start_failed, result.subline)
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

    @Test fun problemsAreOnlyPresentedInTheErrorState() {
        val problems = listOf(HomeProblem(R.string.home_problem_carplay, "previous failure"))
        for (snapshot in listOf(ready, ready.copy(identityReady = false), ready.copy(started = true),
            ready.copy(phase = SessionPhase.CONNECTING), ready.copy(phase = SessionPhase.RECONNECTING),
            ready.copy(phase = SessionPhase.ACTIVE), ready.copy(phase = SessionPhase.ENDED))) {
            val result = presentHome(snapshot.copy(problems = problems))
            assertNotEquals(StatusTone.ERROR, result.tone)
            assertTrue(result.problems.isEmpty())
        }
    }

    @Test fun errorWithoutSpecificProblemsHasOneLocalizedFallbackRow() {
        for (snapshot in listOf(ready.copy(failure = true),
            ready.copy(webReason = WebStartReason(WebStartFailure.OTHER, 8443)))) {
            val result = presentHome(snapshot)
            assertEquals(StatusTone.ERROR, result.tone)
            assertEquals(listOf(HomeProblem(R.string.home_problem_connection)), result.problems)
            assertNull(result.problems.single().text)
        }
    }

    @Test fun webStartFailuresExplainPortConflictsAndAutomaticRetries() {
        for (both in listOf(false, true)) {
            val busy = presentHome(ready.copy(phase = SessionPhase.ACTIVE,
                settings = settings.copy(carlinkOutput = both, bothOutputs = both),
                webReason = WebStartReason(WebStartFailure.PORT_BUSY, 9443)))
            assertEquals(R.string.ui_error, busy.headline)
            assertEquals(StatusTone.ERROR, busy.tone)
            assertEquals(R.string.ui_web_port_busy, busy.subline)
            assertEquals(listOf("9443"), busy.sublineArguments)
        }
        val other = presentHome(ready.copy(webReason = WebStartReason(WebStartFailure.OTHER, 8443)))
        assertEquals(R.string.ui_error, other.headline)
        assertEquals(R.string.ui_web_start_failed, other.subline)
        assertTrue(other.sublineArguments.isEmpty())
        val disabled = presentHome(ready.copy(settings = settings.copy(carlinkOutput = true),
            webReason = WebStartReason(WebStartFailure.PORT_BUSY, 8443)))
        assertNotEquals(StatusTone.ERROR, disabled.tone)
    }

    @Test fun listeningWithoutCarFacingNetworkExplainsWifiOrHotspot() {
        val missing = ready.copy(phase = SessionPhase.ACTIVE, webListening = true, carNetworkAvailable = false)
        assertEquals(R.string.ui_head_unit_network, presentHome(missing).subline)
        assertEquals(R.string.ui_showing_browser, presentHome(missing.copy(carNetworkAvailable = true)).subline)
        assertEquals(R.string.ui_showing_carlink, presentHome(missing.copy(settings = settings.copy(carlinkOutput = true))).subline)
    }

    @Test fun allSixStatusCardsAndReconnectMapToPlainLanguage() {
        val cases = listOf(
            ready.copy(identityReady = false) to (R.string.ui_setup_needed to StatusTone.ATTENTION),
            ready to (R.string.ui_ready to StatusTone.IDLE),
            ready.copy(started = true) to (R.string.ui_waiting to StatusTone.PROGRESS),
            ready.copy(phase = SessionPhase.CONNECTING) to (R.string.ui_connecting to StatusTone.PROGRESS),
            ready.copy(phase = SessionPhase.RECONNECTING) to (R.string.ui_reconnecting to StatusTone.PROGRESS),
            ready.copy(phase = SessionPhase.ACTIVE) to (R.string.ui_running to StatusTone.RUNNING),
            ready.copy(failure = true) to (R.string.ui_error to StatusTone.ERROR),
        )
        cases.forEach { (snapshot, expected) ->
            val result = presentHome(snapshot)
            assertEquals(expected.first, result.headline)
            assertEquals(expected.second, result.tone)
        }
        assertEquals(R.string.ui_showing_browser, presentHome(ready.copy(phase = SessionPhase.ACTIVE)).subline)
        assertEquals(R.string.ui_showing_carlink, presentHome(ready.copy(phase = SessionPhase.ACTIVE,
            settings = settings.copy(carlinkOutput = true))).subline)
        assertEquals(R.string.ui_showing_both, presentHome(ready.copy(phase = SessionPhase.ACTIVE,
            settings = settings.copy(bothOutputs = true))).subline)
    }

    @Test fun everyConnectionStatusRowSuppliesHeadlineHintToneAndArguments() {
        val deviceName = "Relay Bluetooth Name"
        connectionStatusCases(deviceName).forEach { expected ->
            val snapshot = ready.copy(started = true, phase = expected.phase, stage = expected.stage,
                wireless = expected.wireless, deviceName = deviceName)
            val result = presentHome(snapshot)
            assertEquals(expected.toString(), expected.headline, result.headline)
            assertEquals(expected.toString(), expected.subline, result.subline)
            assertEquals(expected.toString(), StatusTone.PROGRESS, result.tone)
            assertEquals(expected.toString(), expected.arguments, result.sublineArguments)
            assertTrue(result.running)
            if (expected.phase in listOf(SessionPhase.CONNECTING, SessionPhase.RECONNECTING)) {
                assertEquals(result, presentHome(snapshot.copy(started = false)))
            }
        }
        val fallback = presentHome(ready.copy(started = true))
        assertEquals(listOf(settings.carlinkDeviceName), fallback.sublineArguments)
    }

    @Test fun earlierStatusBranchesWinOverEveryConnectionStage() {
        for (stage in CarPlayStage.entries) {
            val connecting = ready.copy(started = true, stage = stage, deviceName = "Private name")
            val error = presentHome(connecting.copy(failure = true))
            assertEquals(R.string.ui_error, error.headline)
            assertEquals(R.string.ui_retry_hint, error.subline)
            assertEquals(StatusTone.ERROR, error.tone)
            assertTrue(error.sublineArguments.isEmpty())
            val setup = presentHome(connecting.copy(identityReady = false))
            assertEquals(R.string.ui_setup_needed, setup.headline)
            assertEquals(R.string.ui_setup_hint, setup.subline)
            assertEquals(StatusTone.ATTENTION, setup.tone)
            assertTrue(setup.sublineArguments.isEmpty())
            val checking = presentHome(connecting.copy(checking = true))
            assertEquals(R.string.direlay_checking_identity, checking.subline)
            assertTrue(checking.sublineArguments.isEmpty())
            val active = presentHome(connecting.copy(phase = SessionPhase.ACTIVE))
            assertEquals(R.string.ui_running, active.headline)
            assertEquals(R.string.ui_showing_browser, active.subline)
            assertEquals(StatusTone.RUNNING, active.tone)
            assertTrue(active.sublineArguments.isEmpty())
            val network = presentHome(connecting.copy(webListening = true, carNetworkAvailable = false))
            assertEquals(R.string.ui_head_unit_network, network.subline)
            assertTrue(network.sublineArguments.isEmpty())
            val busy = presentHome(connecting.copy(webReason = WebStartReason(WebStartFailure.PORT_BUSY, 9443)))
            assertEquals(R.string.ui_error, busy.headline)
            assertEquals(R.string.ui_web_port_busy, busy.subline)
            assertEquals(listOf("9443"), busy.sublineArguments)
        }
    }

    @Test fun stageDoesNotTurnAnIdleOrStoppedHomeIntoAConnectionAttempt() {
        for (stage in CarPlayStage.entries) for (phase in listOf(null, SessionPhase.ENDED)) {
            val stopped = ready.copy(phase = phase, stage = stage)
            val idle = presentHome(stopped)
            assertEquals(R.string.ui_ready, idle.headline)
            assertEquals(R.string.ui_start_hint, idle.subline)
            assertEquals(StatusTone.IDLE, idle.tone)
            assertFalse(idle.running)
            assertTrue(idle.sublineArguments.isEmpty())
            assertEquals(R.string.ui_automatic_paused,
                presentHome(stopped.copy(autoStartPaused = true)).subline)
        }
    }

    @Test fun modeReflectsAllChosenModesRatherThanSessionDiagnostics() {
        for (network in WirelessNetworkChoice.entries) for (carlink in listOf(false, true)) for (wireless in listOf(false, true)) {
            val result = presentHome(ready.copy(wireless = wireless, settings = settings.copy(carlinkOutput = carlink,
                wirelessNetwork = settings.wirelessNetwork.copy(choice = network))))
            // USB CarPlay uses no wireless network, so it shows no network row.
            assertEquals(listOfNotNull(if (carlink) R.string.ui_carlink else R.string.ui_browser,
                if (wireless) R.string.direlay_wireless else R.string.ui_usb, setupNetworkLabel(network).takeIf { wireless }),
                result.mode.map { it.value })
        }
    }

    @Test fun checklistOnlyContainsMissingStepsAndEachHasAnAction() {
        assertTrue(presentHome(ready).checklist.isEmpty())
        val missing = presentHome(ready.copy(permissionsReady = false, identityReady = false,
            iphoneSelection = IphoneSelection.NoIphone, validSettings = false))
        assertEquals(listOf(SetupAction.PERMISSIONS, SetupAction.IDENTITY, SetupAction.SETTINGS, SetupAction.IPHONE),
            missing.checklist.map { it.action })
        assertTrue(missing.checklist.all { it.actionLabel != 0 })
        assertFalse(missing.canStart)
        assertFalse(presentHome(ready.copy(checking = true)).canStart)
        assertFalse(presentHome(ready.copy(iphoneSelection = IphoneSelection.NoIphone)).canStart)
        assertEquals(R.string.ui_setup_needed, presentHome(ready.copy(iphoneSelection = IphoneSelection.NoIphone)).headline)
        assertTrue(presentHome(ready.copy(wireless = false, iphoneSelection = IphoneSelection.NoIphone)).checklist.isEmpty())
    }

    @Test fun unresolvedPhoneSelectionExplainsPairingOrOpensPickerAndBlocksStart() {
        val device = BondedDevice("00:11:22:33:A9:30", "Phone", true, true, false)
        val cases = listOf(
            IphoneSelection.NoIphone to (R.string.iphone_pair to SetupAction.IPHONE),
            IphoneSelection.SelectedUnpaired(device.address) to (R.string.iphone_unpaired to SetupAction.CHOOSE_IPHONE),
            IphoneSelection.MultipleConnected(listOf(device)) to (R.string.iphone_multiple to SetupAction.CHOOSE_IPHONE),
            IphoneSelection.MultipleCandidates(listOf(device)) to (R.string.iphone_multiple to SetupAction.CHOOSE_IPHONE),
        )
        cases.forEach { (selection, expected) ->
            val result = presentHome(ready.copy(iphoneSelection = selection))
            assertFalse(result.canStart)
            assertEquals(expected.first, result.checklist.single().text)
            assertEquals(expected.second, result.checklist.single().action)
        }
        assertTrue(presentHome(ready.copy(wireless = false, iphoneSelection = IphoneSelection.NoIphone)).canStart)
    }

    @Test fun endedSessionIsReadyAndActiveSessionOffersStopEvenIfPermissionWasRevoked() {
        assertFalse(presentHome(ready.copy(phase = SessionPhase.ENDED)).running)
        assertTrue(presentHome(ready.copy(phase = SessionPhase.ACTIVE, permissionsReady = false)).running)
    }

    @Test fun pausedAutomaticExplainsHowToResumeOnlyWhileIdle() {
        val paused = ready.copy(autoStartPaused = true)
        assertEquals(R.string.ui_automatic_paused, presentHome(paused).subline)
        assertEquals(R.string.ui_start_hint, presentHome(paused.copy(settings = settings.copy(autoStartWhenReady = false))).subline)
        assertEquals(R.string.ui_showing_browser, presentHome(paused.copy(phase = SessionPhase.ACTIVE)).subline)
        assertNotEquals(R.string.ui_automatic_paused, presentHome(paused.copy(started = true)).subline)
    }
    @Test fun sharedWifiSetupBlocksStartAndExplainsUnreadableNames() {
        val missing = presentHome(ready.copy(sharedWifiReady = false, currentWifi = "Unsaved"))
        assertFalse(missing.canStart)
        assertTrue(missing.checklist.contains(SetupItem(R.string.shared_wifi_not_saved, R.string.shared_wifi_save_current, SetupAction.SHARED_WIFI)))
        val unreadable = presentHome(ready.copy(sharedWifiReady = false))
        assertFalse(unreadable.canStart)
        assertTrue(unreadable.checklist.contains(SetupItem(R.string.shared_wifi_unreadable, R.string.shared_wifi_use, SetupAction.SHARED_WIFI)))
        assertTrue(presentHome(ready.copy(sharedWifiReady = true)).canStart)
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

    @Test fun carPlayFailureCarriesItsExplanationTrailTimeAndPrecedesHostFailures() {
        val error = org.openxplay.direlay.relay.CarPlayError("connection failed", "first_tcp_timeout",
            listOf("12:00:00.000 status=DiscoveringMfi", "+1.000s status=Failed"), 123L)
        val rows = homeProblems(listOf(RelayFailure(null, "connection failed"),
            RelayFailure(null, "different host failure"), RelayFailure("web", "connection failed")),
            false, "", null, null, "", carPlayError = error)
        assertEquals(3, rows.size)
        assertEquals(HomeProblem(R.string.home_problem_carplay, error.text, error.trail.joinToString("\n"),
            R.string.error_explain_first_tcp_timeout, atMillis = 123L), rows.first())
        assertEquals("different host failure", rows[1].text)
        assertEquals(R.string.home_problem_browser, rows[2].title)
        val home = presentHome(ready.copy(failure = true, problems = rows))
        assertEquals(StatusTone.ERROR, home.tone)
        assertEquals(rows, home.problems)
    }
    @Test fun knownCarPlayStartupFailuresHaveLocalizedExplanations() {
        assertEquals(R.string.error_explain_hotspot_not_ready, carPlayFailureExplanation("hotspot_not_ready"))
        assertEquals(R.string.error_explain_hotspot_configuration, carPlayFailureExplanation("hotspot_configuration"))
        assertEquals(R.string.error_explain_first_tcp_timeout, carPlayFailureExplanation("first_tcp_timeout"))
        assertEquals(R.string.error_explain_bluetooth_unusable, carPlayFailureExplanation("bluetooth_unusable"))
        assertNull(carPlayFailureExplanation(null))
        assertNull(carPlayFailureExplanation("unknown"))
    }

}
