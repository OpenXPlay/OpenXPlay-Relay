// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-24
package org.openxplay.direlay.relay

import com.shilapi.xcertplay.network.WirelessStartupPolicy
import com.shilapi.xcertplay.orchestration.CarPlayEventObserver
import com.shilapi.xcertplay.orchestration.CarPlayStatus
import com.shilapi.xcertplay.transport.BluetoothLinkCheck
import com.shilapi.xcertplay.transport.BluetoothRadioCheck
import org.openxplay.direlay.path.LinkFailure
import org.openxplay.direlay.path.LinkStep
import org.openxplay.stream.core.SessionPhase
import java.util.Locale

/** One immutable publication for callbacks on DiPlay, Stream and UI threads. */
data class IphoneLinkSnapshot(val step: LinkStep = LinkStep.IDLE, val sinceMillis: Long? = null,
    val detail: String? = null, val failure: LinkFailure? = null, val sessionSinceMillis: Long? = null)

class IphoneLinkTracker(private val wireless: () -> Boolean = { true },
    private val diagnostics: () -> CarPlayError? = { null },
    private val wallClock: () -> Long = System::currentTimeMillis) : CarPlayEventObserver {
    @Volatile var snapshot = IphoneLinkSnapshot()
        private set
    private var failures = 0
    private var videoSince: Long? = null

    @Synchronized fun reset() { snapshot = IphoneLinkSnapshot(); failures = 0; videoSince = null }

    private fun move(step: LinkStep, detail: String? = null) {
        snapshot = snapshot.copy(step = step, sinceMillis = if (snapshot.step == step) snapshot.sinceMillis else wallClock(), detail = detail)
    }

    @Synchronized override fun onStatus(status: CarPlayStatus) {
        if (status is CarPlayStatus.Failed) {
            resetStableBudget()
            val error = diagnostics()
            val code = classifyIphoneFailure(status.message, error?.code ?: status.startupFailure?.name?.lowercase(Locale.ROOT), status.wifiResetRequired)
            val retryable = wireless() && !status.wifiResetRequired && code !in setOf(
                "hotspot_configuration", "bluetooth_unusable", "wifi_reset_required", "usb_trust_denied", "iphone_locked")
            val retrying = retryable && ++failures <= WirelessStartupPolicy.MAX_STARTUP_RETRIES
            val step = when (code) {
                "hotspot_not_ready", "hotspot_configuration", "wifi_off", "wifi_reset_required" -> LinkStep.STARTING
                "bluetooth_off", "bluetooth_unusable", "iphone_unpaired", "rfcomm_timeout" -> LinkStep.BLUETOOTH
                "first_tcp_timeout" -> LinkStep.JOINING
                "usb_trust_denied", "iphone_locked" -> LinkStep.USB_TRUST
                else -> snapshot.step
            }
            val at = error?.atMillis ?: wallClock()
            val failure = LinkFailure(step, code, error?.text ?: redactFailureText(status.message), at, retrying,
                java.util.Collections.unmodifiableList(ArrayList(error?.trail.orEmpty())))
            snapshot = snapshot.copy(step = step, sinceMillis = at, detail = null, failure = failure)
            videoSince = null
            return
        }
        val step = when (status) {
            CarPlayStatus.DiscoveringMfi, CarPlayStatus.WaitingForMfi, CarPlayStatus.RequestingMfiPermission,
            CarPlayStatus.MfiReady, CarPlayStatus.StartingHotspot, is CarPlayStatus.HotspotReady -> LinkStep.STARTING
            CarPlayStatus.WaitingForPairedIphone -> LinkStep.WAITING_IPHONE
            CarPlayStatus.ConnectingBluetooth -> LinkStep.BLUETOOTH
            CarPlayStatus.RunningWireless -> LinkStep.AUTH
            CarPlayStatus.WirelessActive, CarPlayStatus.WirelessActiveFallback -> LinkStep.JOINING
            CarPlayStatus.DiscoveringIphone, CarPlayStatus.WaitingForIphone -> LinkStep.USB_DETECT
            CarPlayStatus.RequestingIphonePermission -> LinkStep.USB_PERMISSION
            CarPlayStatus.WaitingForReenumeration, CarPlayStatus.SelectingConfiguration,
            CarPlayStatus.OpeningDataPaths -> LinkStep.USB_DETECT
            CarPlayStatus.Pairing -> LinkStep.USB_TRUST
            CarPlayStatus.ConnectingControl, CarPlayStatus.RunningControl -> LinkStep.USB_CONTROL
            CarPlayStatus.AttachingNetwork -> if (wireless()) LinkStep.WAITING_IPHONE else LinkStep.USB_CONTROL
            CarPlayStatus.ControlEnded -> LinkStep.ENDED
            is CarPlayStatus.Failed -> return
        }
        // Wireless handoff notifications may arrive after the session/video callback.
        if (step == LinkStep.JOINING && snapshot.step in setOf(LinkStep.SESSION, LinkStep.VIDEO)) return
        if (step == LinkStep.STARTING || step == LinkStep.ENDED) { resetStableBudget(); videoSince = null }
        move(step)
    }

    @Synchronized override fun onProgress(message: String) {
        if (!wireless() || snapshot.step in setOf(LinkStep.IDLE, LinkStep.ENDED, LinkStep.SESSION, LinkStep.VIDEO)) return
        val progress = when {
            message.startsWith("iap2 identification accepted") -> LinkStep.AUTH to "Identification accepted"
            message.startsWith("iap2 authentication accepted") -> LinkStep.AUTH to "Authentication accepted"
            message.startsWith("iap2 tx=0x5703 ") -> LinkStep.WIFI_SENT to "Wi-Fi details sent"
            message.startsWith("iap2 tx=0x4301 ") -> LinkStep.JOINING to "StartSession sent"
            else -> return
        }
        if (progress.first.ordinal < snapshot.step.ordinal) return
        move(progress.first, progress.second)
    }
    @Synchronized override fun onSessionActive() {
        snapshot = snapshot.copy(failure = null, sessionSinceMillis = snapshot.sessionSinceMillis ?: wallClock())
        if (snapshot.step != LinkStep.VIDEO) move(LinkStep.SESSION)
    }
    @Synchronized override fun onSessionEnded() {
        resetStableBudget(); videoSince = null
        snapshot = snapshot.copy(sessionSinceMillis = null)
        move(LinkStep.ENDED)
    }
    @Synchronized fun onStreamPhase(phase: SessionPhase) {
        when (phase) {
            SessionPhase.ACTIVE -> {
                videoSince = videoSince ?: wallClock()
                snapshot = snapshot.copy(failure = null, sessionSinceMillis = snapshot.sessionSinceMillis ?: wallClock())
                move(LinkStep.VIDEO)
            }
            SessionPhase.ENDED -> onSessionEnded()
            else -> Unit
        }
    }
    private fun resetStableBudget() {
        if (videoSince?.let { wallClock() - it >= WirelessStartupPolicy.STABLE_SESSION_MILLIS } == true) failures = 0
    }
}

/** These markers are DiPlay's public diagnostic messages, never protocol payloads. */
internal fun classifyIphoneFailure(message: String, code: String? = null, wifiResetRequired: Boolean = false): String = when {
    wifiResetRequired -> "wifi_reset_required"
    code != null -> code
    message.contains(BluetoothRadioCheck.RADIO_OFF_MARK, true) -> "bluetooth_off"
    message.contains(BluetoothLinkCheck.NO_SOCKETS_MARK, true) || message.contains(BluetoothLinkCheck.UNREAL_MARK, true) ||
        message.contains("IBluetoothSocketManager", true) || message.contains("get socket manager failed", true) -> "bluetooth_unusable"
    message.contains("no longer paired", true) -> "iphone_unpaired"
    message.contains("RFCOMM", true) && (message.contains("timed out", true) || message.contains("timeout", true)) -> "rfcomm_timeout"
    message.contains("Turn on Wi-Fi", true) -> "wifi_off"
    message.contains("The user denied Lockdown pairing", true) || message.contains("UserDeniedPairing", true) -> "usb_trust_denied"
    message.contains("password protected", true) || message.contains("PasswordProtected", true) -> "iphone_locked"
    else -> "unknown"
}
