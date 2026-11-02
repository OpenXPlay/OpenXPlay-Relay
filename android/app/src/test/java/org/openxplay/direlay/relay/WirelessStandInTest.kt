// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-11-01
package org.openxplay.direlay.relay

import com.shilapi.xcertplay.orchestration.MfiTarget
import org.junit.Assert.*
import org.junit.Test
import org.openxplay.direlay.settings.*
import org.openxplay.stream.core.FrameRatePreference

class WirelessStandInTest {
    private val base = RelaySettings(ConnectionMode.USB, MfiTarget.LOCAL, "", "", "", null,
        FrameRatePreference.AUTO, false, false, false, null)
    private val booleans = listOf(false, true)
    private val outputSets = (1..15).map { mask ->
        RelayOutput.entries.filterIndexed { index, _ -> mask and (1 shl index) != 0 }.toSet()
    }
    private val wirelessSets = setOf(setOf(RelayOutput.DLNA), setOf(RelayOutput.MIRACAST),
        setOf(RelayOutput.DLNA, RelayOutput.MIRACAST))

    @Test fun wirelessOnlyRecognizesExactlyTheThreeWirelessSetsAndRejectsEmpty() {
        assertEquals(4, RelayOutput.entries.size)
        assertEquals(15, outputSets.distinct().size)
        assertEquals(wirelessSets, outputSets.filter(WirelessStandIn::wirelessOnly).toSet())
        assertFalse(WirelessStandIn.wirelessOnly(emptySet()))
        val empty = base.copy(outputs = emptySet())
        assertSame(empty, WirelessStandIn.effective(empty, DeveloperOverrides()))
    }

    @Test fun all15OutputSetsCoverBothAudioSchemesAndEveryDeveloperChoice() {
        for (outputs in outputSets) for (direct in booleans) for (active in booleans)
            for (preview in booleans) for (audio in booleans) for (micEnabled in booleans)
                for (mic in MicrophoneSource.entries) {
                    val developer = DeveloperOverrides(active = active, localPreview = preview,
                        playCarPlayAudio = audio, microphoneSourceEnabled = micEnabled, microphoneSource = mic)
                    val settings = developer.effectiveSettings(base.copy(outputs = outputs, carBluetoothAudio = direct))
                    val effective = WirelessStandIn.effective(settings, developer)
                    val label = "$outputs direct=$direct $developer"
                    if (outputs !in wirelessSets) {
                        assertEquals(label, settings, effective)
                        assertSame(label, settings, effective)
                    } else {
                        assertTrue(label, effective.localPreview)
                        assertEquals(label, settings.playCarPlayAudio || !direct && RelayOutput.MIRACAST in outputs,
                            effective.playCarPlayAudio)
                        val microphone = when {
                            direct -> settings.microphoneSource
                            active && micEnabled -> mic
                            else -> MicrophoneSource.THIS_DEVICE
                        }
                        assertEquals(label, microphone, effective.microphoneSource)
                        assertEquals(label, settings, effective.copy(localPreview = settings.localPreview,
                            playCarPlayAudio = settings.playCarPlayAudio, microphoneSource = settings.microphoneSource))
                        assertEquals(label, effective, WirelessStandIn.effective(effective, developer))
                    }
                    // A policy evaluation cannot change the settings or Developer choices used by the next output set.
                    assertEquals(label, settings,
                        developer.effectiveSettings(base.copy(outputs = outputs, carBluetoothAudio = direct)))
                }
    }

    @Test fun dlnaKeepsExistingPlaybackAndCarDirectKeepsExistingMicrophoneAndPlayback() {
        for (outputs in wirelessSets) for (audio in booleans) for (mic in MicrophoneSource.entries) {
            val settings = base.copy(outputs = outputs, playCarPlayAudio = audio, microphoneSource = mic)
            assertEquals(settings.copy(localPreview = true), WirelessStandIn.effective(settings, DeveloperOverrides()))
            val forwarding = settings.copy(outputs = setOf(RelayOutput.DLNA), carBluetoothAudio = false)
            assertEquals(audio, WirelessStandIn.effective(forwarding, DeveloperOverrides()).playCarPlayAudio)
        }
    }
}
