// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-21
package org.openxplay.direlay.ui

import org.junit.Assert.*
import org.junit.Test
import org.openxplay.direlay.R
import org.openxplay.direlay.clink.ClinkFailureReason
import org.openxplay.direlay.network.*
import org.openxplay.direlay.relay.RelayFailure

class NetworkHomePresentationTest {
    @Test fun modeChoicesOnlyShowStaticRemindersForAnUnreadableRadio() {
        for (wifi in listOf(true, false, null)) for (hotspot in listOf(true, false, null)) for (bluetooth in listOf(true, false, null)) {
            val readings = NetworkReadings(wifi, hotspot, bluetooth = bluetooth)
            assertEquals(if (bluetooth == null) R.string.network_bluetooth_reminder else null,
                networkChoiceReminder(NetworkRequirement.BLUETOOTH, readings))
            assertEquals(R.string.network_open_bluetooth, NetworkRequirement.BLUETOOTH.settingsLabel())
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
            val row = problems.single()
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
}
