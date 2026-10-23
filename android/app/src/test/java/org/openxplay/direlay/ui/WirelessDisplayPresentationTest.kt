// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-22
package org.openxplay.direlay.ui

import com.shilapi.xcertplay.orchestration.MfiTarget
import org.junit.Assert.*
import org.junit.Test
import org.openxplay.direlay.R
import org.openxplay.direlay.miracast.*
import org.openxplay.direlay.relay.RelayFailure
import org.openxplay.direlay.relay.WirelessDisplayFailure
import org.openxplay.direlay.settings.*
import org.openxplay.stream.cast.CastState

class WirelessDisplayPresentationTest {
    private val base = RelaySettings(ConnectionMode.USB, MfiTarget.LOCAL, "", "", "", null, 30, false, false, false, null)
    private val selected = base.copy(dlnaOutput = true, dlnaRenderer = DlnaRendererChoice("uuid:car", "Saved car"))
    private val display = MiracastDisplay(7, "Car display", 1920, 1080, 160, 60.0)

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

    @Test fun eachDlnaStateHasItsOwnHomeLineAndUsesTheResolvedNameWithSavedFallback() {
        val texts = mapOf(CastState.SEARCHING to R.string.home_dlna_searching,
            CastState.WAITING_FOR_VIDEO to R.string.home_dlna_waiting, CastState.STARTING to R.string.home_dlna_starting,
            CastState.PLAYING to R.string.home_dlna_playing, CastState.PAUSED to R.string.home_dlna_paused,
            CastState.STOPPED_BY_RENDERER to R.string.home_dlna_stopped, CastState.ENDED to R.string.home_dlna_waiting)
        for (state in CastState.entries) {
            val line = wirelessDisplayLines(selected, state, "Live car", null, null).single()
            assertEquals(WirelessDisplayLine(R.string.wireless_dlna, texts.getValue(state), listOf("Live car")), line)
            assertEquals(listOf("Saved car"), wirelessDisplayLines(selected, state, null, null, null).single().arguments)
        }
        assertEquals(R.string.home_dlna_searching, wirelessDisplayLines(selected, null, null, null, null).single().text)
        assertEquals(R.string.dlna_choose_receiver,
            wirelessDisplayLines(selected.copy(dlnaRenderer = null), CastState.PLAYING, "Old car", null, null).single().text)
        assertTrue(wirelessDisplayLines(base, CastState.PLAYING, "Car", MiracastState.SHOWING, display).isEmpty())
    }

    @Test fun miracastHomeLinesIncludePermissionAndPhysicalDisplaySize() {
        val settings = base.copy(miracastOutput = true)
        for (state in listOf(null, MiracastState.WAITING_FOR_DISPLAY, MiracastState.ENDED))
            assertEquals(WirelessDisplayLine(R.string.wireless_miracast, R.string.home_miracast_waiting),
                wirelessDisplayLines(settings, null, null, state, null).single())
        assertEquals(R.string.miracast_allow_overlay,
            wirelessDisplayLines(settings, null, null, MiracastState.NEEDS_PERMISSION, display).single().text)
        assertEquals(WirelessDisplayLine(R.string.wireless_miracast, R.string.home_miracast_showing,
            listOf("Car display", "1920", "1080")), wirelessDisplayLines(settings, null, null, MiracastState.SHOWING, display).single())
        assertEquals(R.string.home_miracast_waiting,
            wirelessDisplayLines(settings, null, null, MiracastState.SHOWING, null).single().text)
        assertEquals(listOf(R.string.wireless_dlna, R.string.wireless_miracast),
            wirelessDisplayLines(selected.copy(miracastOutput = true), CastState.PLAYING, null, MiracastState.SHOWING, display).map { it.title })
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
