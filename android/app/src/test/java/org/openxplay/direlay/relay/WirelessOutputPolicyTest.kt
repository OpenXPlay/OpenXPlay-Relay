// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-22
package org.openxplay.direlay.relay

import org.openxplay.stream.core.FrameRatePreference
import com.shilapi.xcertplay.orchestration.MfiTarget
import org.junit.Assert.*
import org.junit.Test
import org.openxplay.direlay.settings.*
import org.openxplay.direlay.miracast.MiracastPicture
import org.openxplay.stream.core.*

class WirelessOutputPolicyTest {
    private val base = RelaySettings(ConnectionMode.USB, MfiTarget.LOCAL, "", "", "", null, FrameRatePreference.AUTO, false, false, false, null)
    private val receiver = DlnaRendererChoice("uuid:car", "Car")

    @Test fun wirelessOnlyDefaultDeclares1080And240DpiWithoutChangingTheCastTransport() {
        val settings = base.copy(outputs = setOf(RelayOutput.DLNA), dlnaRenderer = receiver)
        val plan = checkNotNull(WirelessOutputPolicy.dlna(settings))
        assertEquals(DlnaOutputPlan(receiver.udn, WirelessPictureSize(1920, 1080), 240), plan)
        val caps = OutputCapabilities(LatencyClass.BUFFERED, listOf(DecoderSupport("avc1.64002a", true)),
            mapOf(ScreenId.MAIN to DisplayInfo(1920, 1080, 160, 30.0)), 0, emptySet(), emptySet(), false)
        val inner = RecordingOutput("cast", caps)
        val memory = CarDisplayMemory()
        val output = memory.exclude(WirelessOutputPolicy.declareDlna(inner, plan))
        val forwarded = mutableListOf<OutputCapabilities>()
        output.open(object : OutputHost {
            override fun capabilities(capabilities: OutputCapabilities) { forwarded.add(capabilities) }
            override fun control(event: ControlEvent) {}
            override fun stats(stats: OutputStats) {}
        })
        assertEquals("cast", output.id)
        assertEquals(caps.copy(displays = mapOf(ScreenId.MAIN to DisplayInfo(1920, 1080, 240, 30.0))), forwarded.single())
        assertNull(memory.measuredDisplay(forwarded.single()))
        output.close(); assertTrue(inner.closed)
    }

    @Test fun dlnaNeedsBothTheSwitchAndAReceiverWhileMiracastNeedsOnlyItsSwitch() {
        for (dlna in listOf(false, true)) for (miracast in listOf(false, true))
            for (choice in listOf(null, receiver))
                for (carlink in listOf(false, true)) for (both in listOf(false, true)) {
                    val settings = base.copy(outputs = OutputSelection.legacy(carlink, both, dlna, miracast), dlnaRenderer = choice)
                    assertEquals(if (dlna && choice != null) DlnaOutputPlan(choice.udn, WirelessPictureSize(1280, 720)) else null,
                        WirelessOutputPolicy.dlna(settings))
                    assertEquals(if (miracast) MiracastPicture() else null, WirelessOutputPolicy.miracast(settings))
                }
    }

    @Test fun resolvedSizeAndCustomPictureChangesRebuildOnlyTheAffectedOutput() {
        val settings = base.copy(outputs = setOf(RelayOutput.BROWSER, RelayOutput.DLNA, RelayOutput.MIRACAST), dlnaRenderer = receiver)
        val first = WirelessOutputPolicy.dlna(settings)
        val dlna = RetiringOutputPolicy<DlnaOutputPlan>()
        assertEquals(OutputChange.ADD, dlna.update(first))
        val measured = WirelessPictureResolution(WirelessPictureSize(1920, 720), WirelessPictureSource.BROWSER)
        assertEquals(OutputChange.REMOVE, dlna.update(WirelessOutputPolicy.dlna(settings, measured)))
        val custom = settings.copy(wirelessPicture = WirelessPictureMode.CUSTOM, wirelessPictureSize = WirelessPictureSize(1280, 480))
        assertEquals(WirelessPictureSize(1280, 480), WirelessOutputPolicy.dlna(custom)?.size)
        assertEquals(MiracastPicture(Viewport(1280, 480)), WirelessOutputPolicy.miracast(custom))
        assertEquals(WirelessOutputPolicy.dlna(custom), WirelessOutputPolicy.dlna(custom.copy(miracastStretch = true)))
        assertEquals(MiracastPicture(size = null, stretch = true), WirelessOutputPolicy.miracast(settings.copy(miracastStretch = true)))
        val miracast = RetiringOutputPolicy<MiracastPicture>()
        assertEquals(OutputChange.ADD, miracast.update(WirelessOutputPolicy.miracast(settings)))
        assertEquals(OutputChange.REMOVE, miracast.update(WirelessOutputPolicy.miracast(custom)))
        miracast.ended()
        assertEquals(OutputChange.ADD, miracast.update(WirelessOutputPolicy.miracast(custom)))
        assertEquals(OutputChange.REMOVE, miracast.update(WirelessOutputPolicy.miracast(custom.copy(miracastStretch = true))))
    }

    @Test fun onlyUdnAndPictureChangesRebuildDlna() {
        val settings = base.copy(outputs = setOf(RelayOutput.BROWSER, RelayOutput.DLNA), dlnaRenderer = receiver)
        val policy = RetiringOutputPolicy<DlnaOutputPlan>()
        assertEquals(OutputChange.ADD, policy.update(WirelessOutputPolicy.dlna(settings)))
        for (same in listOf(settings, settings.copy(dlnaRenderer = receiver.copy(name = "New name")),
            settings.copy(outputs = settings.outputs + setOf(RelayOutput.CARLINK, RelayOutput.MIRACAST), frameRate = FrameRatePreference.FPS_60)))
            assertEquals(OutputChange.KEEP, policy.update(WirelessOutputPolicy.dlna(same)))
        for (changed in listOf(settings.copy(wirelessPicture = WirelessPictureMode.CUSTOM, wirelessPictureSize = WirelessPictureSize(1920, 1080)),
            settings.copy(dlnaRenderer = receiver.copy(udn = "uuid:new")))) {
            assertEquals(OutputChange.REMOVE, policy.update(WirelessOutputPolicy.dlna(changed)))
            assertTrue(policy.retiring)
            assertEquals(OutputChange.KEEP, policy.update(WirelessOutputPolicy.dlna(changed)))
            policy.ended()
            assertEquals(OutputChange.ADD, policy.update(WirelessOutputPolicy.dlna(settings)))
        }
    }

    @Test fun retiringWaitsForEndedAndThenUsesTheLatestChoice() {
        val policy = RetiringOutputPolicy<DlnaOutputPlan>()
        val original = DlnaOutputPlan("uuid:first", WirelessPictureSize(1280, 720))
        val replacement = DlnaOutputPlan("uuid:latest", WirelessPictureSize(1920, 1080))
        assertEquals(OutputChange.KEEP, policy.update(null))
        assertEquals(OutputChange.ADD, policy.update(original))
        assertEquals(OutputChange.REMOVE, policy.update(replacement))
        for (requested in listOf(null, original, replacement)) {
            assertEquals(OutputChange.KEEP, policy.update(requested))
            assertEquals(original, policy.current)
        }
        policy.ended()
        assertEquals(OutputChange.ADD, policy.update(replacement))
        assertEquals(replacement, policy.current)
        assertEquals(OutputChange.REMOVE, policy.update(null))
        policy.ended()
        assertEquals(OutputChange.KEEP, policy.update(null))
        assertNull(policy.current)
    }

    @Test fun miracastRapidOffOnDoesNotOverlapOutputsAndDisabledOutputsStayRemoved() {
        for (reenable in listOf(false, true)) {
            val policy = RetiringOutputPolicy<MiracastPicture>()
            assertEquals(OutputChange.ADD, policy.update(MiracastPicture()))
            assertEquals(OutputChange.KEEP, policy.update(MiracastPicture()))
            assertEquals(OutputChange.REMOVE, policy.update(null))
            val latest = MiracastPicture(Viewport(1920, 720), stretch = true).takeIf { reenable }
            repeat(3) { assertEquals(OutputChange.KEEP, policy.update(latest)) }
            policy.ended()
            assertEquals(if (reenable) OutputChange.ADD else OutputChange.KEEP, policy.update(latest))
        }
    }
}
