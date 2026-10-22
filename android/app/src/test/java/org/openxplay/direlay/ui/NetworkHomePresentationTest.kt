// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-21
package org.openxplay.direlay.ui

import com.shilapi.xcertplay.orchestration.MfiTarget
import org.junit.Assert.*
import org.junit.Test
import org.openxplay.direlay.R
import org.openxplay.direlay.clink.ClinkFailureReason
import org.openxplay.direlay.network.*
import org.openxplay.direlay.relay.RelayFailure
import org.openxplay.direlay.settings.*
import org.openxplay.stream.core.SessionPhase

class NetworkHomePresentationTest {
    private val settings = RelaySettings(ConnectionMode.USB, MfiTarget.LOCAL, "", "", "", null,
        30, false, true, false, null, carlinkOutput = true)
    private val ready = HomeSnapshot(settings, wireless = false, permissionsReady = true, identityReady = true)

    @Test fun modeChoicesOnlyShowStaticRemindersForAnUnreadableRadio() {
        for (wifi in listOf(true, false, null)) for (hotspot in listOf(true, false, null)) {
            val readings = NetworkReadings(wifi, hotspot)
            assertEquals(if (wifi == null) R.string.network_wifi_reminder else null,
                networkChoiceReminder(NetworkRequirement.WIFI, readings))
            assertEquals(if (hotspot == null) R.string.network_hotspot_reminder else null,
                networkChoiceReminder(NetworkRequirement.HOTSPOT, readings))
        }
    }

    @Test fun wifiOffAndEveryJoinFailureCarryFriendlyTextActionAndRawDetail() {
        val codes = ClinkFailureReason.entries.filter { it.name.startsWith("P2P_") || it.name.startsWith("WIFI_") }
        for (code in codes) {
            val error = "Carlink failed\nCaused by: ${code.code}"
            val problems = homeProblems(listOf(RelayFailure("clink", "clink: raw failure")), true, code.code,
                error, null, "")
            val row = presentHome(ready.copy(failure = true, problems = problems)).problems.single()
            assertEquals(R.string.home_problem_carlink, row.title)
            assertEquals(R.string.network_wifi_problem, row.textResource)
            assertEquals(NetworkRequirement.WIFI, row.action)
            assertEquals(R.string.network_open_wifi, row.action!!.settingsLabel())
            assertEquals("${code.code}\n$error\nclink: raw failure", row.detail)
        }
        assertEquals(R.string.network_wifi_problem,
            homeProblems(emptyList(), true, "auth_rejected", null, null, "",
                NetworkReadiness(wifi = Readiness.PROBLEM)).single().textResource)
        assertEquals(R.string.network_wifi_problem,
            homeProblems(listOf(RelayFailure("clink", "clink: open failed")), false, "", null, null, "",
                NetworkReadiness(wifi = Readiness.PROBLEM)).single().textResource)
        assertTrue(homeProblems(emptyList(), false, "wifi_off", "previous failure", null, "").isEmpty())
    }

    @Test fun hotspotOffIsOnlyShownForAnObservedProblemAndFailureAndClearsOnRecovery() {
        for (state in Readiness.entries) for (failed in listOf(false, true)) {
            val problems = homeProblems(emptyList(), failed, "auth_rejected", null, null, "",
                NetworkReadiness(hotspot = state))
            val row = problems.firstOrNull { it.title == R.string.home_problem_hotspot }
            if (state == Readiness.PROBLEM && failed) {
                assertEquals(R.string.network_hotspot_problem, row!!.textResource)
                assertEquals(NetworkRequirement.HOTSPOT, row.action)
                assertEquals(R.string.network_open_hotspot, row.action!!.settingsLabel())
            } else assertNull(row)
        }
        val problems = homeProblems(listOf(RelayFailure(null, "Input: start failed")), false, "", null, null, "",
            NetworkReadiness(hotspot = Readiness.PROBLEM))
        assertEquals(listOf(R.string.home_problem_carplay, R.string.home_problem_hotspot), problems.map { it.title })
        assertEquals(listOf(R.string.home_problem_browser), homeProblems(listOf(RelayFailure("web", "web: output failed")),
            false, "", null, null, "", NetworkReadiness(hotspot = Readiness.PROBLEM)).map { it.title })
    }

    @Test fun usbRoleHintBelongsOnlyToAWiredWaitAndClearsAsSoonAsTheDeviceIsListed() {
        val readings = NetworkReadings(usb = UsbConnectionState.DEVICE, usbDevicesEmpty = true)
        val waiting = ready.copy(started = true, usbRoleProblem = wiredUsbRoleProblem(false, true, readings))
        val row = presentHome(waiting).hints.single()
        assertEquals(R.string.home_problem_usb, row.title)
        assertEquals(R.string.network_usb_role_problem, row.textResource)
        assertEquals("USB_STATE: connected=true, host_connected=false; USB devices=[]", row.detail)
        assertNull(row.action)
        assertTrue(presentHome(waiting.copy(usbRoleProblem = wiredUsbRoleProblem(false, true,
            readings.copy(usbDevicesEmpty = false)))).hints.isEmpty())
        assertTrue(presentHome(waiting.copy(usbRoleProblem = false)).hints.isEmpty())
        assertTrue(presentHome(waiting.copy(started = false)).hints.isEmpty())
        assertTrue(presentHome(waiting.copy(phase = SessionPhase.ACTIVE)).hints.isEmpty())
        assertTrue(presentHome(waiting.copy(wireless = true)).hints.isEmpty())
        assertEquals(R.string.ui_usb_hint, presentHome(waiting.copy(usbRoleProblem = false)).subline)
    }

    @Test fun staticRemindersAppearOnlyDuringAManualUnknownConnection() {
        val requirements = NetworkRequirement.entries.toSet()
        val unknown = ready.copy(started = true, networkReadiness = NetworkReadiness(Readiness.UNKNOWN, Readiness.UNKNOWN),
            manualNetworkReminders = requirements)
        assertEquals(listOf(R.string.network_wifi_reminder, R.string.network_hotspot_reminder), presentHome(unknown).reminders)
        assertTrue(presentHome(unknown.copy(manualNetworkReminders = emptySet())).reminders.isEmpty())
        assertTrue(presentHome(unknown.copy(networkReadiness = NetworkReadiness())).reminders.isEmpty())
        assertTrue(presentHome(unknown.copy(phase = SessionPhase.ACTIVE)).reminders.isEmpty())
        assertTrue(presentHome(unknown.copy(started = false)).reminders.isEmpty())
        assertTrue(presentHome(unknown.copy(failure = true)).reminders.isEmpty())
    }
}
