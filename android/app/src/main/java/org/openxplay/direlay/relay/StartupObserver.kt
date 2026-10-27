// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-27
package org.openxplay.direlay.relay

import com.shilapi.xcertplay.orchestration.CarPlayEventObserver
import com.shilapi.xcertplay.orchestration.CarPlayStatus
import org.openxplay.direlay.bridge.DiPlayInput

/** The relay shell translates host events into the Stream session reason contract. */
internal class StartupObserver(private val input: DiPlayInput) : CarPlayEventObserver {
    override fun onStatus(status: CarPlayStatus) {
        val reason = when (status) {
            is CarPlayStatus.ConnectingBluetooth, is CarPlayStatus.ConnectingControl -> "connecting"
            is CarPlayStatus.Pairing, is CarPlayStatus.RunningControl, is CarPlayStatus.RunningWireless -> "authenticating"
            is CarPlayStatus.Failed -> "failed"
            is CarPlayStatus.ControlEnded -> "disconnected"
            else -> return
        }
        input.startupProgress("startup/$reason")
    }
    override fun onProgress(message: String) {
        when {
            message == "iap2 authentication accepted" || message.startsWith("iap2 tx=0x5703") ||
                message == "iap2 tx=0x4301 carplay-start-session" -> input.startupProgress("startup/joining-network")
        }
    }
    override fun onSessionActive() { input.startupProgress("startup/waiting-media") }
    override fun onSessionEnded() { input.startupProgress("startup/disconnected") }
}
