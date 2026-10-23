// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-23
package org.openxplay.direlay.relay

import com.shilapi.xcertplay.orchestration.CarPlayEventObserver
import com.shilapi.xcertplay.orchestration.CarPlayStatus
import org.openxplay.stream.core.SessionPhase

enum class CarPlayStage { NONE, WAITING, BLUETOOTH, BLUETOOTH_CONNECTED, USB_CONNECTED, SESSION, INTERRUPTED }

/** Each input replaces the stage; callbacks and readers may use different threads. */
class CarPlayStageTracker : CarPlayEventObserver {
    @Volatile var stage = CarPlayStage.NONE
        private set

    fun reset() { stage = CarPlayStage.NONE }

    override fun onStatus(status: CarPlayStatus) {
        stage = when (status) {
            CarPlayStatus.DiscoveringMfi, CarPlayStatus.WaitingForMfi,
            CarPlayStatus.RequestingMfiPermission, CarPlayStatus.MfiReady,
            CarPlayStatus.StartingHotspot, is CarPlayStatus.HotspotReady -> CarPlayStage.NONE
            CarPlayStatus.WaitingForPairedIphone, CarPlayStatus.DiscoveringIphone,
            CarPlayStatus.WaitingForIphone -> CarPlayStage.WAITING
            CarPlayStatus.ConnectingBluetooth -> CarPlayStage.BLUETOOTH
            CarPlayStatus.RunningWireless, CarPlayStatus.WirelessActive,
            CarPlayStatus.WirelessActiveFallback -> CarPlayStage.BLUETOOTH_CONNECTED
            CarPlayStatus.RequestingIphonePermission, CarPlayStatus.WaitingForReenumeration,
            CarPlayStatus.SelectingConfiguration, CarPlayStatus.OpeningDataPaths,
            CarPlayStatus.Pairing, CarPlayStatus.ConnectingControl,
            CarPlayStatus.AttachingNetwork, CarPlayStatus.RunningControl -> CarPlayStage.USB_CONNECTED
            CarPlayStatus.ControlEnded -> CarPlayStage.INTERRUPTED
            is CarPlayStatus.Failed -> return
        }
    }

    override fun onSessionActive() { stage = CarPlayStage.SESSION }
    override fun onSessionEnded() { stage = CarPlayStage.INTERRUPTED }
    fun onStreamPhase(phase: SessionPhase) {
        if (phase == SessionPhase.ENDED) stage = CarPlayStage.INTERRUPTED
    }
}
