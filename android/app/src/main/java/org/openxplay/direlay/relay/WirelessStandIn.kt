// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-11-01
package org.openxplay.direlay.relay

import org.openxplay.direlay.settings.*

/** Wireless displays return no touch or microphone; the relay phone stands in for the car. */
object WirelessStandIn {
    fun wirelessOnly(outputs: Set<RelayOutput>): Boolean =
        outputs.isNotEmpty() && RelayOutput.BROWSER !in outputs && RelayOutput.CARLINK !in outputs && RelayOutput.CARLIFE !in outputs

    /** [settings] already includes the user's Developer choices; derived values are never persisted. */
    fun effective(settings: RelaySettings, developer: DeveloperOverrides): RelaySettings {
        if (!wirelessOnly(settings.outputs)) return settings
        val forwarding = !settings.carBluetoothAudio
        return settings.copy(
            localPreview = true,
            playCarPlayAudio = settings.playCarPlayAudio || forwarding && settings.miracastOutput,
            microphoneSource = if (forwarding && !(developer.active && developer.microphoneSourceEnabled))
                MicrophoneSource.THIS_DEVICE else settings.microphoneSource,
        )
    }
}
