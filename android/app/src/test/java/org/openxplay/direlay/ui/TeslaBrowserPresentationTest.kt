// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-24
package org.openxplay.direlay.ui

import org.junit.Assert.*
import org.junit.Test
import org.openxplay.direlay.path.*

class TeslaBrowserPresentationTest {
    private val input = PathInput(setup = SetupInput(network = NetworkPlan.USB,
        outputs = setOf(CarOutput.BROWSER, CarOutput.TESLA)),
        relay = RelayInput(identity = IdentityState.READY),
        iphone = IphoneInput(usb = UsbRole.HOST, started = true, step = LinkStep.VIDEO))

    @Test fun teslaFailureBelongsToItsStepWhileTheSessionKeepsRunning() {
        val raw = "IOException: address failed"
        val path = presentHostOverlay(HostOverlaySnapshot(input.copy(car = CarInput(tesla = OutputInput(OutputPhase.FAILED, code = raw))))).path
        assertTrue(path.running)
        val step = path.steps.first { it.id == StepId.TESLA }
        assertEquals(StepState.FAILED, step.state)
        assertEquals(raw, step.code)
        assertEquals(StepState.OK, path.steps.first { it.id == StepId.SESSION }.state)
    }

    @Test fun teslaEvidenceStaysSeparateFromTheTrustedFirstPageAddresses() {
        val tesla = "https://3.3.3.1:9443/"
        val name = "https://relay.example:9443/"
        val lan = "https://192.168.43.1:9443/"
        val result = presentHostOverlay(HostOverlaySnapshot(input.copy(car = CarInput(pageUrl = name,
            tesla = OutputInput(OutputPhase.READY, tesla))), listOf(name, lan, name)))
        assertEquals(listOf(name, lan), result.receiverUrls)
        assertEquals(tesla, result.path.steps.first { it.id == StepId.TESLA }.evidence?.raw)
        assertEquals(name, result.path.steps.first { it.id == StepId.CAR_PAGE }.evidence?.raw)
    }
}
