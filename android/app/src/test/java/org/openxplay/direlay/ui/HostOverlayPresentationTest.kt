// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-24
package org.openxplay.direlay.ui

import org.junit.Assert.*
import org.junit.Test
import org.openxplay.direlay.R
import org.openxplay.direlay.path.*

class HostOverlayPresentationTest {
    private val ready = PathInput(nowMillis = 10_000, version = "0.1.6",
        setup = SetupInput(network = NetworkPlan.PHONE_HOTSPOT),
        relay = RelayInput(identity = IdentityState.READY, bluetoothOn = true, hotspotOn = true),
        car = CarInput(address = "192.168.43.1", web = WebServerInput(WebPhase.LISTENING, 9443),
            pageUrl = "https://192.168.43.1:9443/"), iphone = IphoneInput(pick = IphonePick.PICKED))

    @Test fun overlayUsesTheSamePathWithoutLosingItsVersion() {
        val result = presentHostOverlay(HostOverlaySnapshot(ready, highPerformance = false))
        assertEquals(presentPath(ready), result.path)
        assertEquals(ready.version, result.path.version)
        assertFalse(result.highPerformance)
    }

    @Test fun waitingForTheCarOffersStartNowOnTheFocusStep() {
        val input = ready.copy(iphone = ready.iphone.copy(started = true, carWaitUntilMillis = 30_000))
        val path = presentHostOverlay(HostOverlaySnapshot(input)).path
        assertEquals(StepId.CARPLAY_START, focusForPage(path)?.id)
        assertEquals(StepAction.START_NOW, focusForPage(path)?.action)
        assertEquals(StepState.WAITING, focusForPage(path)?.state)
        assertTrue(path.running)
    }

    @Test fun failuresOfferRetryButAutomaticRetriesStayWorking() {
        val failure = LinkFailure(LinkStep.JOINING, "first_tcp_timeout", "timeout", 9000, false, listOf("trail"))
        val input = ready.copy(iphone = ready.iphone.copy(started = true, step = LinkStep.JOINING, failure = failure))
        val failed = presentHostOverlay(HostOverlaySnapshot(input))
        assertEquals(PathTone.ERROR, failed.path.tone)
        assertEquals(StepAction.RETRY, failed.path.focus?.action)
        assertEquals("first_tcp_timeout", failed.path.focus?.code)
        val stage = presentStage(failed.path, StageFlags(overlay = true))
        assertEquals(StageAction.Step(StepAction.RETRY, failed.path.focus!!.id, failed.path.focus!!.actionLabel), stage.primary)
        assertEquals(StageAction.Stop, stage.secondary)
        val retrying = presentHostOverlay(HostOverlaySnapshot(input.copy(iphone = input.iphone.copy(failure = failure.copy(retrying = true)))))
        assertEquals(PathTone.PROGRESS, retrying.path.tone)
        assertEquals(StepState.WORKING, retrying.path.steps.first { it.id == StepId.IPHONE_WIFI }.state)
        assertNull(presentStage(retrying.path, StageFlags(overlay = true)).primary)
    }

    @Test fun aLivePictureKeepsNamesChipsAndDiagnostic() {
        val input = ready.copy(iphone = ready.iphone.copy(started = true, step = LinkStep.VIDEO, name = "iPhone 15"),
            live = LiveInput(true, 1000, listOf("1920×720", "60 fps", "H.265")))
        val path = presentHostOverlay(HostOverlaySnapshot(input)).path
        assertEquals(PathTone.LIVE, path.tone)
        assertEquals(input.live.chips, path.live?.chips)
        assertEquals("iPhone 15", path.nodes.iphone)
        assertEquals(presentPath(input).diagnostic, path.diagnostic)
    }

    @Test fun noticesKeepSourceFlagsAndUseTheSharedPriority() {
        val input = ready.copy(iphone = ready.iphone.copy(started = true))
        val result = presentHostOverlay(HostOverlaySnapshot(input, displayChanged = true,
            carIncomplete = true, setupCompleted = false))
        assertTrue(result.displayChanged)
        assertTrue(result.carIncomplete)
        assertFalse(result.setupCompleted)
        assertEquals(StageNotice.DISPLAY_CHANGED, presentStage(result.path, StageFlags(overlay = true,
            displayChanged = result.displayChanged, carIncomplete = result.carIncomplete,
            setupCompleted = result.setupCompleted)).notice)
    }

    @Test fun previewDoesNotConstructTheOverlayOrPollItsRuntime() {
        var calls = 0
        assertNull(hostOverlayUnlessPreview(true) { calls++; "view" })
        assertEquals(0, calls)
        assertEquals("view", hostOverlayUnlessPreview(false) { calls++; "view" })
        assertEquals(1, calls)
    }

    @Test fun sheetUrlsKeepTrustedFirstAndFailureEvidenceIsRetained() {
        val urls = listOf("https://relay.example/", "https://192.168.43.1/", "https://relay.example/")
        val problems = listOf(HomeProblem(R.string.home_problem_carplay, "raw"))
        val result = presentHostOverlay(HostOverlaySnapshot(ready, urls, problems = problems))
        assertEquals(urls.take(2), result.receiverUrls)
        assertEquals(problems, result.problems)
    }
}
