// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-24
package org.openxplay.direlay.relay

import com.shilapi.xcertplay.orchestration.ManualHotspotBand
import org.junit.Assert.*
import org.junit.Test
import org.openxplay.direlay.bluetooth.BondedDevice
import org.openxplay.direlay.bluetooth.IphoneSelection
import org.openxplay.direlay.clink.ClinkStage
import org.openxplay.direlay.miracast.MiracastState
import org.openxplay.direlay.path.*
import org.openxplay.direlay.settings.*
import org.openxplay.stream.cast.CastState
import org.openxplay.stream.core.*
import java.util.Locale

class PathInputsTest {
    private val phone = BondedDevice("00:11:22:33:44:55", "iPhone 15", true, false, false)
    @Test fun setupOutputsFollowTheSelectionAndAddOnlyTheTeslaRoute() {
        val settings = RelaySettings(ConnectionMode.USB, com.shilapi.xcertplay.orchestration.MfiTarget.LOCAL, "", "", "", null,
            autoStartOnBoot = false, autoStartWhenReady = true, localPreview = false, webPort = null)
        for (mask in 1..15) for (tesla in TeslaBrowserMode.entries) {
            val outputs = RelayOutput.entries.filterIndexed { index, _ -> mask and (1 shl index) != 0 }.toSet()
            val selected = settings.copy(outputs = outputs, teslaBrowser = tesla)
            val expected = outputs.map { CarOutput.valueOf(it.name) }.toSet() +
                if (tesla == TeslaBrowserMode.OFF) emptySet() else setOf(CarOutput.TESLA)
            assertEquals(expected, pathOutputs(selected))
        }
    }

    @Test fun onlyBondedRequiresIphoneEvidenceAndExplicitChoicesStillWork() {
        val headphones = phone.copy(name = "Headphones", phoneClass = false)
        assertEquals(IphonePick.NOT_IPHONE, iphonePick(IphoneSelection.OnlyBonded(headphones)))
        assertEquals("Headphones", iphoneName(IphoneSelection.OnlyBonded(headphones)))
        assertEquals(IphonePick.PICKED, iphonePick(IphoneSelection.OnlyBonded(headphones.copy(iap2 = true))))
        assertEquals(IphonePick.PICKED, iphonePick(IphoneSelection.OnlyBonded(phone.copy(name = "My IPHONE"))))
        assertEquals(IphonePick.NONE, iphonePick(IphoneSelection.NoIphone))
        assertEquals(IphonePick.UNPAIRED, iphonePick(IphoneSelection.SelectedUnpaired(phone.address)))
        assertEquals(IphonePick.MULTIPLE, iphonePick(IphoneSelection.MultipleCandidates(listOf(phone))))
        assertEquals(IphonePick.MULTIPLE, iphonePick(IphoneSelection.MultipleConnected(listOf(phone))))
        for (selection in listOf(IphoneSelection.Selected(headphones), IphoneSelection.Connected(phone),
            IphoneSelection.OnlyCandidate(phone))) {
            assertEquals(IphonePick.PICKED, iphonePick(selection))
            assertNotNull(iphoneName(selection))
        }
        assertNull(iphoneName(IphoneSelection.NoIphone))
        assertNull(iphoneName(IphoneSelection.SelectedUnpaired(phone.address)))
        assertNull(iphoneName(IphoneSelection.MultipleCandidates(listOf(phone))))
    }
    @Test fun carlinkCombinesActualStatesAndPublishedSubStages() {
        val states = mapOf("Disabled" to CarlinkPhase.OFF, "Requires Android 10+" to CarlinkPhase.UNSUPPORTED,
            "Waiting for permissions" to CarlinkPhase.NEEDS_PERMISSIONS, "Blocked" to CarlinkPhase.BLOCKED,
            "ENDED" to CarlinkPhase.ENDED, "NEGOTIATING" to CarlinkPhase.NEGOTIATING, "PLAYING" to CarlinkPhase.PLAYING)
        for ((state, phase) in states) for (stage in ClinkStage.entries) for (pin in listOf(false, true))
            assertEquals(phase, carlinkPhase(state, stage, pin))
        val exchanging = setOf(ClinkStage.BLE_PEER, ClinkStage.GATT_CONNECT, ClinkStage.GATT_CONNECTED,
            ClinkStage.GATT_SERVICES, ClinkStage.BLE_EXCHANGE)
        val joining = setOf(ClinkStage.WIFI_JOIN, ClinkStage.TCP_CONNECT)
        val auth = setOf(ClinkStage.AUTH, ClinkStage.AUTH_PINNED, ClinkStage.AUTH_SUCCESS, ClinkStage.TCP_CONNECTED)
        for (stage in ClinkStage.entries) {
            val phase = when (stage) {
                in exchanging -> CarlinkPhase.EXCHANGING
                in joining -> CarlinkPhase.JOINING
                in auth -> CarlinkPhase.AUTHENTICATING
                ClinkStage.AUTH_PIN, ClinkStage.AUTH_CONFIRM -> CarlinkPhase.PIN
                else -> CarlinkPhase.SCANNING
            }
            assertEquals(stage.name, phase, carlinkPhase("CONNECTING", stage, false))
            assertEquals(CarlinkPhase.PIN, carlinkPhase("AUTHENTICATING", stage, true))
            assertEquals(if (phase == CarlinkPhase.SCANNING) CarlinkPhase.AUTHENTICATING else phase,
                carlinkPhase("AUTHENTICATING", stage, false))
        }
        assertEquals(CarlinkPhase.SCANNING, carlinkPhase("CONNECTING", null, false))
    }
    @Test fun webPhaseIsTypedAndOnlyFailuresCarryCodes() {
        for (phase in WebPhase.entries) for (reason in listOf(null,
            WebStartReason(WebStartFailure.PORT_BUSY, 9443), WebStartReason(WebStartFailure.OTHER, 9443))) {
            val input = webPathInput(phase, 9443, reason, "car.example")
            assertEquals(phase, input.phase)
            assertEquals(9443, input.port)
            assertEquals("car.example", input.trustedName)
            assertEquals(if (phase == WebPhase.FAILED) {
                if (reason?.failure == WebStartFailure.PORT_BUSY) "web_port_busy" else "web_failed"
            } else null, input.code)
        }
    }
    @Test fun wirelessOutputsDistinguishSelectionWaitingVideoAndShowing() {
        val failure = WirelessDisplayFailure("renderer_stopped", "detail", 123)
        assertEquals(OutputPhase.OFF, dlnaPathInput(false, true, CastState.PLAYING, "car", failure).phase)
        assertEquals(OutputPhase.WAITING_USER, dlnaPathInput(true, false, null, null, null).phase)
        val phases = mapOf(CastState.SEARCHING to OutputPhase.SEARCHING, CastState.WAITING_FOR_VIDEO to OutputPhase.WAITING_VIDEO,
            CastState.STARTING to OutputPhase.STARTING, CastState.PLAYING to OutputPhase.PLAYING)
        for (state in CastState.entries) assertEquals(phases[state] ?: OutputPhase.READY,
            dlnaPathInput(true, true, state, "car", null).phase)
        assertEquals(OutputPhase.READY, dlnaPathInput(true, true, null, "car", null).phase)
        assertEquals(45L, dlnaPathInput(true, true, CastState.PLAYING, "car", null, 45).sinceMillis)
        assertEquals(46L, miracastPathInput(true, MiracastState.SHOWING, "car", null, 46).sinceMillis)
        assertEquals(47L, teslaPathInput(true, "https://car/", null, 47).sinceMillis)
        assertEquals(OutputInput(OutputPhase.FAILED, "car", "renderer_stopped", 123),
            dlnaPathInput(true, true, CastState.PLAYING, "car", failure))
        for (state in MiracastState.entries) assertEquals(
            if (state == MiracastState.SHOWING) OutputPhase.PLAYING else OutputPhase.WAITING_USER,
            miracastPathInput(true, state, "display", null).phase)
        assertEquals(OutputPhase.WAITING_USER, miracastPathInput(true, null, null, null).phase)
        assertEquals(OutputPhase.OFF, miracastPathInput(false, MiracastState.SHOWING, "display", failure).phase)
        assertEquals(OutputInput(OutputPhase.FAILED, "display", "renderer_stopped", 123),
            miracastPathInput(true, MiracastState.SHOWING, "display", failure))
        assertEquals(OutputPhase.OFF, teslaPathInput(false, "https://car/", "failed").phase)
        assertEquals(OutputPhase.STARTING, teslaPathInput(true, null, null).phase)
        assertEquals(OutputPhase.READY, teslaPathInput(true, "https://car/", null).phase)
        assertEquals("failed", teslaPathInput(true, null, "failed").code)
        assertEquals(OutputPhase.FAILED, teslaPathInput(true, null, "failed").phase)
    }
    @Test fun pictureAndLiveChipsUseCompactLocaleIndependentFacts() {
        assertEquals("1920×720", pictureSize(1920, 720))
        assertNull(pictureSize(0, 720))
        assertNull(pictureSize(1920, -1))
        assertEquals("1280×720 H.265", negotiatedPicture(1280, 720, VideoCodec.H265))
        assertEquals("1280×720 H.264", negotiatedPicture(1280, 720, VideoCodec.H264))
        assertEquals("1280×720", negotiatedPicture(1280, 720, null))
        assertNull(negotiatedPicture(0, 0, VideoCodec.H264))
        assertEquals("2.4 GHz", hotspotBand(ManualHotspotBand.GHZ_2_4))
        assertEquals("5 GHz", hotspotBand(ManualHotspotBand.GHZ_5))
        assertNull(hotspotBand(ManualHotspotBand.AUTO))
        val format = VideoFormat.create(ScreenId.MAIN, VideoCodec.H264, org.openxplay.direlay.bridge.avcConfig, fpsHint = 60.0)
        val values = VideoMeasurement.Values(59.8, 8_100_000.0, 0.0, 0, 60, 0)
        val locale = Locale.getDefault()
        try {
            Locale.setDefault(Locale.GERMANY)
            assertEquals(listOf("${format.width}×${format.height}", "45 fps", "H.264", "8.1 Mbps"), liveChips(format, values.copy(fps = 45.0)))
            assertEquals(listOf("${format.width}×${format.height}", "60 fps", "H.264"), liveChips(format, null))
        } finally { Locale.setDefault(locale) }
        assertTrue(liveChips(null, null).isEmpty())
        assertTrue(liveChips(null, values.copy(fps = Double.NaN, bitsPerSecond = -1.0)).isEmpty())
    }
}
