// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-24
package org.openxplay.direlay.ui

import org.junit.Assert.*
import org.junit.Test
import org.openxplay.direlay.R
import org.openxplay.direlay.path.*
import org.openxplay.direlay.settings.TeslaBrowserMode

class TeslaBrowserPresentationTest {
    @Test fun modeOptionsPutTeslaMirrorFirstAndOffLastWithoutChangingTheEnum() {
        assertEquals(listOf(TeslaBrowserMode.TESLAMIRROR, TeslaBrowserMode.VIRTUAL_NAME, TeslaBrowserMode.PUBLIC_ADDRESS,
            TeslaBrowserMode.TESLA_ANDROID, TeslaBrowserMode.OFF), teslaBrowserOptions.map { it.first })
        assertEquals(TeslaBrowserMode.entries.toSet(), teslaBrowserOptions.map { it.first }.toSet())
        assertEquals(listOf(R.string.tesla_mirror, R.string.tesla_virtual_name, R.string.tesla_public_address,
            R.string.tesla_android, R.string.tesla_off), teslaBrowserOptions.map { it.second.title })
        assertEquals(listOf(R.string.tesla_virtual_name_hint, R.string.tesla_virtual_name_hint, R.string.tesla_public_address_hint,
            R.string.tesla_public_address_hint, R.string.tesla_intro), teslaBrowserOptions.map { it.second.supporting })
        assertEquals(TeslaBrowserMode.OFF, TeslaBrowserMode.entries.first())
    }

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
