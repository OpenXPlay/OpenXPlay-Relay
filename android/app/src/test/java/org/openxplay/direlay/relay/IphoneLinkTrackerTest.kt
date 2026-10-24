// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-24
package org.openxplay.direlay.relay

import com.shilapi.xcertplay.network.WirelessStartupFailure
import com.shilapi.xcertplay.orchestration.CarPlayStatus
import com.shilapi.xcertplay.transport.BluetoothRadioCheck
import com.shilapi.xcertplay.transport.BluetoothLinkCheck
import com.shilapi.xcertplay.transport.LockdownPairingException
import org.junit.Assert.*
import org.junit.Test
import org.openxplay.direlay.path.LinkStep
import org.openxplay.stream.core.SessionPhase
import org.openxplay.stream.core.SessionState
import org.openxplay.stream.core.SessionId
import org.openxplay.stream.core.SessionConfig

class IphoneLinkTrackerTest {
    private var now = 100L
    private var wireless = true
    private var diagnostic: CarPlayError? = null
    private val tracker = IphoneLinkTracker({ wireless }, { diagnostic }, { now })
    private fun status(status: CarPlayStatus, step: LinkStep) {
        now++
        tracker.onStatus(status)
        assertEquals(status.toString(), step, tracker.snapshot.step)
    }
    @Test fun wirelessStatusAndProgressSequenceDoesNotRegressOnHandoffOrRetransmission() {
        assertEquals(LinkStep.IDLE, tracker.snapshot.step)
        for (s in listOf(CarPlayStatus.DiscoveringMfi, CarPlayStatus.WaitingForMfi,
            CarPlayStatus.RequestingMfiPermission, CarPlayStatus.MfiReady, CarPlayStatus.StartingHotspot,
            CarPlayStatus.HotspotReady("secret", "5 GHz", 36, "", "", "manual"))) status(s, LinkStep.STARTING)
        status(CarPlayStatus.WaitingForPairedIphone, LinkStep.WAITING_IPHONE)
        status(CarPlayStatus.AttachingNetwork, LinkStep.WAITING_IPHONE)
        status(CarPlayStatus.ConnectingBluetooth, LinkStep.BLUETOOTH)
        status(CarPlayStatus.RunningWireless, LinkStep.AUTH)
        tracker.onProgress("iap2 identification accepted")
        assertEquals("Identification accepted", tracker.snapshot.detail)
        val since = tracker.snapshot.sinceMillis
        now++
        tracker.onProgress("iap2 authentication accepted")
        assertEquals("Authentication accepted", tracker.snapshot.detail)
        assertEquals(since, tracker.snapshot.sinceMillis)
        tracker.onProgress("iap2 tx=0x5703 accessory-wifi-configuration")
        assertEquals(LinkStep.WIFI_SENT, tracker.snapshot.step)
        tracker.onProgress("iap2 tx=0x4301 carplay-start-session")
        assertEquals(LinkStep.JOINING, tracker.snapshot.step)
        tracker.onProgress("iap2 tx=0x5703 post-transport accessory-wifi-configuration")
        assertEquals(LinkStep.JOINING, tracker.snapshot.step)
        tracker.onSessionActive()
        assertEquals(LinkStep.SESSION, tracker.snapshot.step)
        status(CarPlayStatus.WirelessActive, LinkStep.SESSION)
        tracker.onStreamPhase(SessionPhase.ACTIVE)
        assertEquals(LinkStep.VIDEO, tracker.snapshot.step)
        val active = tracker.snapshot
        tracker.onProgress("iap2 identification accepted")
        status(CarPlayStatus.WirelessActiveFallback, LinkStep.VIDEO)
        assertEquals(active, tracker.snapshot)
        tracker.onStreamPhase(SessionPhase.ENDED)
        assertEquals(LinkStep.ENDED, tracker.snapshot.step)
        assertNull(tracker.snapshot.sessionSinceMillis)
    }
    @Test fun onlyStableProgressPrefixesAreAcceptedAndTheyNeverExposePayloads() {
        status(CarPlayStatus.RunningWireless, LinkStep.AUTH)
        val before = tracker.snapshot
        listOf("iap2 rx=0x5703 secret", "identification accepted", "x iap2 authentication accepted",
            "iap2 tx=0x57030 secret", "iap2 tx=0x4e0e secret").forEach(tracker::onProgress)
        assertEquals(before, tracker.snapshot)
        tracker.onProgress("iap2 tx=0x5703 password=private")
        assertEquals("Wi-Fi details sent", tracker.snapshot.detail)
        assertFalse(tracker.snapshot.toString().contains("private"))
    }
    @Test fun wiredStatusesFindTrustAndControlWithoutBluetoothOrAutomaticRetries() {
        wireless = false
        status(CarPlayStatus.DiscoveringMfi, LinkStep.STARTING)
        for (s in listOf(CarPlayStatus.DiscoveringIphone, CarPlayStatus.WaitingForIphone,
            CarPlayStatus.WaitingForReenumeration, CarPlayStatus.SelectingConfiguration, CarPlayStatus.OpeningDataPaths))
            status(s, LinkStep.USB_DETECT)
        status(CarPlayStatus.RequestingIphonePermission, LinkStep.USB_PERMISSION)
        status(CarPlayStatus.Pairing, LinkStep.USB_TRUST)
        tracker.onProgress("iap2 tx=0x4301 carplay-start-session")
        assertEquals(LinkStep.USB_TRUST, tracker.snapshot.step)
        for (s in listOf(CarPlayStatus.ConnectingControl, CarPlayStatus.AttachingNetwork, CarPlayStatus.RunningControl))
            status(s, LinkStep.USB_CONTROL)
        tracker.onSessionActive()
        assertEquals(LinkStep.SESSION, tracker.snapshot.step)
        status(CarPlayStatus.ControlEnded, LinkStep.ENDED)
        tracker.onStatus(CarPlayStatus.Failed("broken"))
        assertFalse(tracker.snapshot.failure!!.retrying)
    }
    @Test fun everyKnownUncodedFailureUsesItsStepAndPreservesRedactedDiagnostics() {
        val cases = listOf(
            Triple(BluetoothRadioCheck.RADIO_OFF, "bluetooth_off", LinkStep.BLUETOOTH),
            Triple(BluetoothRadioCheck.NO_ADAPTER, "bluetooth_unusable", LinkStep.BLUETOOTH),
            Triple(BluetoothLinkCheck.UNREAL_MARK, "bluetooth_unusable", LinkStep.BLUETOOTH),
            Triple(BluetoothLinkCheck.NO_SOCKETS_MARK, "bluetooth_unusable", LinkStep.BLUETOOTH),
            Triple("IBluetoothSocketManager was null", "bluetooth_unusable", LinkStep.BLUETOOTH),
            Triple("bt get socket manager failed", "bluetooth_unusable", LinkStep.BLUETOOTH),
            Triple("The selected iPhone is no longer paired. Choose it again in DiPlay.", "iphone_unpaired", LinkStep.BLUETOOTH),
            Triple("RFCOMM connect timed out after 15000ms", "rfcomm_timeout", LinkStep.BLUETOOTH),
            Triple("RFCOMM timeout", "rfcomm_timeout", LinkStep.BLUETOOTH),
            Triple("Turn on Wi-Fi", "wifi_off", LinkStep.STARTING),
            Triple(LockdownPairingException.UserDeniedPairing.message!!, "usb_trust_denied", LinkStep.USB_TRUST),
            Triple("UserDeniedPairing", "usb_trust_denied", LinkStep.USB_TRUST),
            Triple(LockdownPairingException.PasswordProtected.message!!, "iphone_locked", LinkStep.USB_TRUST),
            Triple("PasswordProtected", "iphone_locked", LinkStep.USB_TRUST),
            Triple("unrecognized failure", "unknown", LinkStep.AUTH))
        for ((message, code, step) in cases) {
            tracker.reset()
            tracker.onStatus(CarPlayStatus.RunningWireless)
            diagnostic = CarPlayError("safe", null, listOf("trail"), 55)
            tracker.onStatus(CarPlayStatus.Failed(message))
            val failure = tracker.snapshot.failure!!
            assertEquals(message, code, failure.code)
            assertEquals(step, failure.step)
            assertEquals("safe", failure.text)
            assertEquals(listOf("trail"), failure.trail)
            assertEquals(55L, failure.atMillis)
        }
    }
    @Test fun codedFailuresResetRequiredAndSixthRetryableFailureStopRetrying() {
        for (reason in WirelessStartupFailure.entries) {
            tracker.reset()
            diagnostic = null
            tracker.onStatus(CarPlayStatus.Failed("coded", startupFailure = reason))
            assertEquals(reason.name.lowercase(), tracker.snapshot.failure!!.code)
            assertEquals(reason.retryable, tracker.snapshot.failure!!.retrying)
        }
        tracker.reset()
        diagnostic = CarPlayError("safe", "first_tcp_timeout", listOf("line"), 500)
        repeat(6) { attempt ->
            tracker.onStatus(CarPlayStatus.Failed("coded"))
            assertEquals(attempt < 5, tracker.snapshot.failure!!.retrying)
            assertEquals(LinkStep.JOINING, tracker.snapshot.failure!!.step)
            tracker.onStatus(CarPlayStatus.DiscoveringMfi)
            assertNotNull(tracker.snapshot.failure)
        }
        tracker.onStatus(CarPlayStatus.Failed("p2p stuck", wifiResetRequired = true))
        assertEquals("wifi_reset_required", tracker.snapshot.failure!!.code)
        assertFalse(tracker.snapshot.failure!!.retrying)
        tracker.reset()
        assertEquals(IphoneLinkSnapshot(), tracker.snapshot)
        tracker.onStatus(CarPlayStatus.Failed("coded"))
        assertTrue(tracker.snapshot.failure!!.retrying)
    }
    @Test fun stableVideoResetsRetryBudgetButAShortSessionDoesNot() {
        repeat(5) { tracker.onStatus(CarPlayStatus.Failed("retry")) }
        tracker.onStreamPhase(SessionPhase.ACTIVE)
        now += 59_999
        tracker.onSessionEnded()
        tracker.onStatus(CarPlayStatus.Failed("retry"))
        assertFalse(tracker.snapshot.failure!!.retrying)
        tracker.onStreamPhase(SessionPhase.ACTIVE)
        now += 60_000
        tracker.onSessionEnded()
        tracker.onStatus(CarPlayStatus.Failed("retry"))
        assertTrue(tracker.snapshot.failure!!.retrying)
    }
    @Test fun relayStatusFeedsActiveAndEndedOnlyOnPhaseTransitions() {
        val status = RelayStatus(iphoneLink = tracker)
        fun phase(phase: SessionPhase) = status.onSession(SessionState(SessionId(1), phase), SessionConfig(emptyMap()))
        phase(SessionPhase.ACTIVE)
        assertEquals(LinkStep.VIDEO, tracker.snapshot.step)
        phase(SessionPhase.ENDED)
        assertEquals(LinkStep.ENDED, tracker.snapshot.step)
        tracker.reset()
        phase(SessionPhase.ENDED)
        assertEquals(LinkStep.IDLE, tracker.snapshot.step)
    }
    @Test fun oldSnapshotsAndFailureTrailsCannotBeChangedByLaterCallbacks() {
        val trail = arrayListOf("first")
        diagnostic = CarPlayError("safe", null, trail, now)
        tracker.onStatus(CarPlayStatus.Failed("Turn on Bluetooth"))
        val failed = tracker.snapshot
        trail.add("later")
        assertEquals(listOf("first"), failed.failure!!.trail)
        try {
            @Suppress("UNCHECKED_CAST")
            (failed.failure.trail as MutableList<String>).add("mutated")
            fail("Failure trail must be immutable")
        } catch (_: UnsupportedOperationException) { }
        tracker.onSessionActive()
        assertNull(tracker.snapshot.failure)
        assertNotNull(failed.failure)
        tracker.reset()
        assertEquals(IphoneLinkSnapshot(), tracker.snapshot)
        assertEquals(LinkStep.BLUETOOTH, failed.step)
    }
}
