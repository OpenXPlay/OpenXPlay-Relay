// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-22
package org.openxplay.direlay.ui

import org.openxplay.stream.core.FrameRatePreference
import com.shilapi.xcertplay.orchestration.MfiTarget
import org.junit.Assert.*
import org.junit.Test
import org.openxplay.direlay.R
import org.openxplay.direlay.relay.RelayFailure
import org.openxplay.direlay.relay.WirelessDisplayFailure
import org.openxplay.direlay.settings.*

class WirelessDisplayPresentationTest {
    private val base = RelaySettings(ConnectionMode.USB, MfiTarget.LOCAL, "", "", "", null, FrameRatePreference.AUTO, false, false, false, null)
    private val selected = base.copy(dlnaOutput = true, dlnaRenderer = DlnaRendererChoice("uuid:car", "Saved car"))

    @Test fun outputSummaryAppendsEachEnabledDisplayAndPreservesEveryBrowserCarlinkChoice() {
        for (carlink in listOf(false, true)) for (both in listOf(false, true))
            for (dlna in listOf(false, true)) for (miracast in listOf(false, true)) {
                val settings = base.copy(carlinkOutput = carlink, bothOutputs = both, dlnaOutput = dlna, miracastOutput = miracast)
                val expected = listOf(if (both) R.string.menu_both_outputs else if (carlink) R.string.ui_carlink else R.string.ui_browser) +
                    listOfNotNull(R.string.wireless_dlna.takeIf { dlna }, R.string.wireless_miracast.takeIf { miracast })
                assertEquals(expected, outputSummaryLabels(settings))
            }
        val labels = mapOf(R.string.ui_browser to "Car browser", R.string.wireless_dlna to "DLNA")
        assertEquals("Car browser · DLNA", outputSummary(selected, labels::getValue))
    }

    @Test fun disablingWirelessOutputsHidesTheirHubFailuresAndKeepsOtherFailures() {
        val dlna = RelayFailure("cast", "DLNA")
        val miracast = RelayFailure("miracast", "Miracast")
        val others = listOf(RelayFailure("web", "Browser"), RelayFailure("clink", "Carlink"), RelayFailure(null, "Input"))
        for (castOn in listOf(false, true)) for (miracastOn in listOf(false, true))
            assertEquals(listOfNotNull(dlna.takeIf { castOn }, miracast.takeIf { miracastOn }) + others,
                visibleHomeFailures(base.copy(dlnaOutput = castOn, miracastOutput = miracastOn), listOf(dlna, miracast) + others))
    }

    @Test fun failureRowsKeepTechnicalDetailsAndTimesWithLocalizedTitlesAndExplanations() {
        val dlna = WirelessDisplayFailure("renderer_rejected", "action=Play code=701", 1234)
        val miracast = WirelessDisplayFailure("window_failed", "WindowManager", 5678)
        val rows = homeProblems(listOf(RelayFailure("cast", "duplicate"), RelayFailure("miracast", "duplicate")),
            false, "", null, null, "", dlnaFailure = dlna, miracastFailure = miracast)
        assertEquals(2, rows.size)
        assertEquals(HomeProblem(R.string.home_problem_dlna, "renderer_rejected", "action=Play code=701",
            R.string.dlna_error_rejected, atMillis = 1234), rows[0])
        assertEquals(HomeProblem(R.string.home_problem_miracast, "window_failed", "WindowManager",
            R.string.miracast_error_window, atMillis = 5678), rows[1])
        val fallback = homeProblems(listOf(RelayFailure("cast", "DLNA failed", 12), RelayFailure("miracast", "Display failed", 34)),
            false, "", null, null, "")
        assertEquals(listOf(R.string.home_problem_dlna, R.string.home_problem_miracast), fallback.map { it.title })
        assertEquals(listOf(12L, 34L), fallback.map { it.atMillis })
        assertNull(wirelessDisplayProblem("cast", dlna.copy(code = "future")).textResource)
    }
}
