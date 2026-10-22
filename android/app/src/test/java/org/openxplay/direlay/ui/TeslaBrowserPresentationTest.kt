// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-22
package org.openxplay.direlay.ui

import com.shilapi.xcertplay.orchestration.MfiTarget
import org.junit.Assert.*
import org.junit.Test
import org.openxplay.direlay.R
import org.openxplay.direlay.settings.*
import org.openxplay.stream.core.SessionPhase

class TeslaBrowserPresentationTest {
    private val settings = RelaySettings(ConnectionMode.USB, MfiTarget.LOCAL, "", "", "", null,
        30, false, true, false, null, teslaBrowser = TeslaBrowserMode.PUBLIC_ADDRESS)
    private val home = HomeSnapshot(settings, wireless = false, permissionsReady = true, identityReady = true,
        phase = SessionPhase.ACTIVE, started = true)

    @Test fun teslaFailureIsAnExpandableProblemWhileCarPlayKeepsRunning() {
        val raw = "IOException: address failed\nCaused by: platform reason"
        val problems = homeProblems(emptyList(), false, "", null, null, "", teslaFailure = raw)
        val presented = presentHome(home.copy(problems = problems))
        assertEquals(StatusTone.RUNNING, presented.tone)
        assertTrue(presented.running)
        assertEquals(R.string.tesla_title, presented.hints.single().title)
        assertEquals(R.string.tesla_address_failed, presented.hints.single().textResource)
        assertEquals(listOf(raw), presented.hints.single().arguments)
        val overlay = presentHostOverlay(HostOverlaySnapshot(home.copy(problems = problems)))
        assertEquals(StatusTone.RUNNING, overlay.tone)
        assertEquals(problems, overlay.problems)
    }

    @Test fun teslaUrlComesBeforeTheTrustedNameAndLanAddressesWithoutDuplicates() {
        val tesla = "https://3.3.3.1:9443/"
        val name = "https://myrelay.duckdns.org:9443/"
        val lan = "https://192.168.43.1:9443/"
        val presented = presentHostOverlay(HostOverlaySnapshot(home, publicNameUrl = name, teslaUrl = tesla,
            receiverUrls = listOf(name, lan, tesla)))
        assertEquals(listOf(tesla, name, lan), presented.receiverUrls)
        assertEquals(tesla, presented.teslaUrl)
        val named = presentHostOverlay(HostOverlaySnapshot(home, publicNameUrl = name, teslaUrl = name,
            receiverUrls = listOf(name, lan)))
        assertEquals(listOf(name, lan), named.receiverUrls)
    }
}
