// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-23
package org.openxplay.direlay.ui

import com.shilapi.xcertplay.orchestration.MfiTarget
import org.junit.Assert.*
import org.junit.Test
import org.openxplay.direlay.R
import org.openxplay.direlay.relay.CarPlayStage
import org.openxplay.direlay.settings.ConnectionMode
import org.openxplay.direlay.settings.RelaySettings
import org.openxplay.stream.core.SessionPhase

class StatusLogDetailTest {
    private val settings = RelaySettings(ConnectionMode.WIRELESS, MfiTarget.LOCAL, "", "", "", null,
        30, false, false, false, null)
    private val snapshot = HomeSnapshot(settings, wireless = true, permissionsReady = true, identityReady = true,
        started = true, stage = CarPlayStage.WAITING, deviceName = "Private Relay Bluetooth Name")
    private val names = mapOf(R.string.ui_waiting to "ui_waiting",
        R.string.ui_waiting_wireless_hint to "ui_waiting_wireless_hint",
        R.string.ui_reconnecting to "ui_reconnecting", R.string.ui_reconnecting_hint to "ui_reconnecting_hint")

    @Test fun fieldOrderAndNullPhaseAreStableAndArgumentsAreNeverLogged() {
        val presentation = presentHome(snapshot)
        assertEquals(listOf(snapshot.deviceName), presentation.sublineArguments)
        val result = statusLogDetail(presentation.headline, presentation.subline, presentation.tone, snapshot, names::getValue)
        assertEquals("headline=ui_waiting subline=ui_waiting_wireless_hint tone=PROGRESS phase=none " +
            "stage=WAITING started=true wireless=true", result)
        assertFalse(result.contains(snapshot.deviceName))
        assertEquals(result, statusLogDetail(presentation.headline, presentation.subline, presentation.tone,
            snapshot.copy(deviceName = "Another private name"), names::getValue))
    }

    @Test fun nonNullPhaseAndFalseFlagsUseTheirExactValues() {
        val value = snapshot.copy(phase = SessionPhase.RECONNECTING, stage = CarPlayStage.INTERRUPTED,
            started = false, wireless = false)
        assertEquals("headline=ui_reconnecting subline=ui_reconnecting_hint tone=PROGRESS phase=RECONNECTING " +
            "stage=INTERRUPTED started=false wireless=false",
            statusLogDetail(R.string.ui_reconnecting, R.string.ui_reconnecting_hint, StatusTone.PROGRESS, value, names::getValue))
    }

    @Test fun missingResourcesFallBackToNumbersAndStoppedHintIsNone() {
        assertEquals("headline=123 subline=456 tone=ERROR phase=none stage=WAITING started=true wireless=true",
            statusLogDetail(123, 456, StatusTone.ERROR, snapshot) { error("Missing resource") })
        assertEquals("headline=host_stopped subline=none tone=IDLE phase=none stage=NONE started=false wireless=false",
            statusLogDetail(R.string.host_stopped, null, StatusTone.IDLE,
                snapshot.copy(stage = CarPlayStage.NONE, started = false, wireless = false)) { "host_stopped" })
    }
}
