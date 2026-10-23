// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-22
package org.openxplay.direlay.relay

import com.shilapi.xcertplay.orchestration.MfiTarget
import org.junit.Assert.*
import org.junit.Test
import org.openxplay.direlay.settings.*
import org.openxplay.direlay.miracast.MiracastPicture
import org.openxplay.stream.core.Viewport

class WirelessOutputPolicyTest {
    private val base = RelaySettings(ConnectionMode.USB, MfiTarget.LOCAL, "", "", "", null, 30, false, false, false, null)
    private val receiver = DlnaRendererChoice("uuid:car", "Car")

    @Test fun dlnaNeedsBothTheSwitchAndAReceiverWhileMiracastNeedsOnlyItsSwitch() {
        for (dlna in listOf(false, true)) for (miracast in listOf(false, true))
            for (choice in listOf(null, receiver)) for (picture in DlnaPicture.entries)
                for (carlink in listOf(false, true)) for (both in listOf(false, true)) {
                    val settings = base.copy(dlnaOutput = dlna, dlnaRenderer = choice, dlnaPicture = picture,
                        miracastOutput = miracast, carlinkOutput = carlink, bothOutputs = both)
                    assertEquals(if (dlna && choice != null) DlnaOutputPlan(choice.udn, if (picture == DlnaPicture.P720) WirelessPictureSize(1280, 720) else WirelessPictureSize(1920, 1080)) else null,
                        WirelessOutputPolicy.dlna(settings))
                    assertEquals(if (miracast) MiracastPicture() else null, WirelessOutputPolicy.miracast(settings))
                }
    }

    @Test fun resolvedSizeAndCustomPictureChangesRebuildOnlyTheAffectedOutput() {
        val settings = base.copy(dlnaOutput = true, dlnaRenderer = receiver, miracastOutput = true)
        val first = WirelessOutputPolicy.dlna(settings)
        val dlna = RetiringOutputPolicy<DlnaOutputPlan>()
        assertEquals(OutputChange.ADD, dlna.update(first))
        val measured = WirelessPictureResolution(WirelessPictureSize(1920, 720), WirelessPictureSource.BROWSER)
        assertEquals(OutputChange.REMOVE, dlna.update(WirelessOutputPolicy.dlna(settings, measured)))
        val custom = settings.copy(wirelessPicture = WirelessPictureMode.CUSTOM, wirelessPictureSize = WirelessPictureSize(1280, 480))
        assertEquals(WirelessPictureSize(1280, 480), WirelessOutputPolicy.dlna(custom)?.size)
        assertEquals(MiracastPicture(Viewport(1280, 480)), WirelessOutputPolicy.miracast(custom))
        assertEquals(WirelessOutputPolicy.dlna(custom), WirelessOutputPolicy.dlna(custom.copy(dlnaPicture = DlnaPicture.P1080, miracastStretch = true)))
        assertEquals(MiracastPicture(size = null, stretch = true), WirelessOutputPolicy.miracast(settings.copy(miracastStretch = true)))
        val miracast = RetiringOutputPolicy<MiracastPicture>()
        assertEquals(OutputChange.ADD, miracast.update(WirelessOutputPolicy.miracast(settings)))
        assertEquals(OutputChange.REMOVE, miracast.update(WirelessOutputPolicy.miracast(custom)))
        miracast.ended()
        assertEquals(OutputChange.ADD, miracast.update(WirelessOutputPolicy.miracast(custom)))
        assertEquals(OutputChange.REMOVE, miracast.update(WirelessOutputPolicy.miracast(custom.copy(miracastStretch = true))))
    }

    @Test fun onlyUdnAndPictureChangesRebuildDlna() {
        val settings = base.copy(dlnaOutput = true, dlnaRenderer = receiver)
        val policy = RetiringOutputPolicy<DlnaOutputPlan>()
        assertEquals(OutputChange.ADD, policy.update(WirelessOutputPolicy.dlna(settings)))
        for (same in listOf(settings, settings.copy(dlnaRenderer = receiver.copy(name = "New name")),
            settings.copy(miracastOutput = true, carlinkOutput = true, fps = 60)))
            assertEquals(OutputChange.KEEP, policy.update(WirelessOutputPolicy.dlna(same)))
        for (changed in listOf(settings.copy(dlnaPicture = DlnaPicture.P1080),
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
