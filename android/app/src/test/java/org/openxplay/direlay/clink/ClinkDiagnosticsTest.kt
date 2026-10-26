// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-15
package org.openxplay.direlay.clink

import org.junit.Assert.*
import org.junit.Test
import org.openxplay.stream.clink.*
import org.openxplay.stream.clink.protocol.AuthIdentity
import java.util.concurrent.CompletionException
import java.util.concurrent.ExecutionException

class ClinkDiagnosticsTest {
    private class Trust(var key: ByteArray? = null) : TrustStore {
        override fun loadIdentity(): AuthIdentity? = null
        override fun saveIdentity(identity: AuthIdentity) = Unit
        override fun loadPeerKey(headUnitId: String): ByteArray? = key
        override fun savePeerKey(headUnitId: String, publicKey: ByteArray) { key = publicKey }
        override fun forgetPeer(headUnitId: String) { key = null }
    }
    @Test fun pinAndConfirmationPathsPreservePortResultsWithoutLoggingSecrets() {
        val events = mutableListOf<ClinkDiagnostic>()
        val pins = object : PinProvider {
            override fun requestPin(headUnitId: String): String { assertEquals("private-id", headUnitId); return "001234" }
            override fun confirmPairing(headUnitId: String) = true
        }
        val session = ClinkSessionDiagnostics(pins, Trust(), ClinkDiagnostics { events.add(it) })
        session.state(ClinkState.CONNECTING)
        session.state(ClinkState.AUTHENTICATING)
        assertNull(session.trustStore.loadPeerKey("private-id"))
        assertEquals("001234", session.pinProvider.requestPin("private-id"))
        assertTrue(session.pinProvider.confirmPairing("private-id"))
        session.state(ClinkState.NEGOTIATING)
        session.state(ClinkState.PLAYING)
        session.state(ClinkState.ENDED)
        assertEquals(listOf(
            ClinkDiagnostic.State(ClinkState.CONNECTING), ClinkStage.OUTPUT_START,
            ClinkDiagnostic.State(ClinkState.AUTHENTICATING), ClinkStage.TCP_CONNECT,
            ClinkStage.TCP_CONNECTED, ClinkDiagnostic.Trusted(false), ClinkStage.AUTH_PIN, ClinkStage.AUTH_CONFIRM,
            ClinkDiagnostic.State(ClinkState.NEGOTIATING), ClinkStage.AUTH_SUCCESS,
            ClinkDiagnostic.State(ClinkState.PLAYING), ClinkDiagnostic.State(ClinkState.ENDED),
            ClinkStage.OUTPUT_STOP), events)
        val lines = events.joinToString { it.line() }
        assertFalse(lines.contains("private-id"))
        assertFalse(lines.contains("001234"))
    }
    @Test fun pinnedPathCanFallBackToPinWithoutExposingKey() {
        val events = mutableListOf<ClinkDiagnostic>()
        val key = byteArrayOf(42, 43, 44)
        val trust = Trust(key)
        val session = ClinkSessionDiagnostics(PinProvider { null }, trust, ClinkDiagnostics { events.add(it) })
        assertSame(key, session.trustStore.loadPeerKey("secret-peer"))
        assertNull(session.pinProvider.requestPin("secret-peer"))
        assertEquals(listOf(ClinkStage.TCP_CONNECTED, ClinkDiagnostic.Trusted(true), ClinkStage.AUTH_PINNED, ClinkStage.AUTH_PIN), events)
        session.trustStore.forgetPeer("secret-peer")
        assertNull(trust.key)
        assertEquals(ClinkDiagnostic.Trusted(false), events.last())
        session.trustStore.savePeerKey("secret-peer", key)
        assertSame(key, trust.key)
        assertEquals(ClinkDiagnostic.Trusted(true), events.last())
    }
    @Test fun distinguishesTcpFailureAndAuthenticationRejection() {
        val events = mutableListOf<ClinkDiagnostic>()
        val session = ClinkSessionDiagnostics(PinProvider { null }, Trust(), ClinkDiagnostics { events.add(it) })
        session.state(ClinkState.AUTHENTICATING)
        session.error(java.net.ConnectException("SSID=secret password=secret"))
        assertEquals(ClinkDiagnostic.Failure(ClinkStage.TCP_CONNECT, "unknown:ConnectException", "ConnectException"), events.last())
        session.trustStore.loadPeerKey("secret-peer")
        session.error(IllegalStateException("auth_rejected"))
        assertEquals(ClinkDiagnostic.Failure(ClinkStage.AUTH, "auth_rejected", "IllegalStateException"), events.last())
        assertFalse(events.joinToString { it.line() }.contains("secret"))
    }
    @Test fun failuresNeverCopyPlatformMessagesOrNestedCredentialCauses() {
        val secret = "ssid=private-ssid psk=private-password pin=001234 mac=01:23:45:67:89:ab"
        val failures = listOf(
            IllegalStateException(secret), SecurityException(secret),
            ExecutionException(CompletionException(IllegalArgumentException(secret))),
            RuntimeException(secret, IllegalStateException("auth_rejected")),
            IllegalStateException("auth_rejected $secret"))
        for (error in failures) {
            val failure = ClinkFailures.describe(ClinkStage.WIFI_JOIN, error)
            assertTrue(failure.code.startsWith("unknown:"))
            assertFalse(failure.line().contains("private"))
            assertFalse(failure.line().contains("001234"))
            assertFalse(failure.line().contains("01:23:45:67:89:ab"))
        }
        assertEquals("unknown:IllegalArgumentException",
            ClinkFailures.describe(ClinkStage.WIFI_JOIN, failures[2]).code)
    }
    @Test fun reasonCodesSurviveFutureWrappingAndCoverP2pFailures() {
        for ((status, code) in mapOf(0 to "wifi_join_p2p_error", 1 to "wifi_join_p2p_unsupported",
            2 to "wifi_join_p2p_busy", 99 to "wifi_join_p2p_unknown")) {
            val error = ExecutionException(ClinkFailure(ClinkFailures.p2p(status)))
            assertEquals(code, ClinkFailures.describe(ClinkStage.WIFI_JOIN, error).code)
        }
        val timeout = ClinkFailures.describe(ClinkStage.BLE_SCAN,
            ExecutionException(ClinkFailure(ClinkFailureReason.BLE_TIMEOUT,
                java.util.concurrent.TimeoutException("secret credentials"))))
        assertEquals("ble_timeout", timeout.code)
        assertEquals("TimeoutException", timeout.exceptionClass)
        assertFalse(timeout.line().contains("secret"))
        assertEquals("ble_exchange_timeout", ClinkFailures.describe(ClinkStage.BLE_EXCHANGE,
            ClinkFailure(ClinkFailureReason.BLE_EXCHANGE_TIMEOUT)).code)
    }
    @Test fun linkLossEmitsSafeFailureAndLossEvent() {
        val events = mutableListOf<ClinkDiagnostic>()
        val session = ClinkSessionDiagnostics(PinProvider { null }, Trust(), ClinkDiagnostics { events.add(it) })
        session.state(ClinkState.NEGOTIATING)
        session.error(java.io.EOFException("vehicle_channel_closed"))
        assertEquals(ClinkDiagnostic.Failure(ClinkStage.SESSION, "vehicle_channel_closed", "EOFException"), events[events.lastIndex - 1])
        assertEquals(ClinkStage.LINK_LOST, events.last())
    }
    @Test fun bleRecoveryDiagnosticsExposeOnlyProtocolChoicesAndOutcomes() {
        assertEquals("ble_request type=1002 subscription=cccd", ClinkDiagnostic.BleRequest(1002, false).line())
        assertEquals("ble_request type=1001 subscription=legacy", ClinkDiagnostic.BleRequest(1001, true).line())
        assertEquals("ble_cccd result=timeout", ClinkDiagnostic.Cccd(ClinkDiagnostic.CccdResult.TIMEOUT).line())
        assertEquals("ble_service_changed state=writing", ClinkDiagnostic.ServiceChanged("writing").line())
        assertEquals("ble_reply type=1002", ClinkDiagnostic.Reply(1002).line())
        assertEquals("ble_request_rejected", ClinkFailures.describe(ClinkStage.BLE_EXCHANGE,
            ClinkFailure(ClinkFailureReason.BLE_REQUEST_REJECTED)).code)
        assertEquals("ble_cccd_timeout", ClinkFailures.describe(ClinkStage.BLE_EXCHANGE,
            ClinkFailure(ClinkFailureReason.BLE_CCCD_TIMEOUT)).code)
    }
    @Test fun successfulExchangeAndJoinOnlyExposeFrequencyAndValidatedAddresses() {
        assertEquals("ble_exchange_done frequency=5180", ClinkDiagnostic.Exchange(5180).line())
        assertEquals("wifi_join_success peer=192.168.49.1 local=192.168.49.2",
            ClinkDiagnostic.Joined("192.168.49.1", "192.168.49.2").line())
        assertEquals("wifi_join_method method=network_specifier reason=carplay_reserved",
            ClinkDiagnostic.JoinChoice(WifiJoinPolicy.Method.NETWORK_SPECIFIER,
                ClinkDiagnostic.JoinReason.CARPLAY_RESERVED).line())
    }
    @Test fun stepFactsExposeOnlyStructuredValues() {
        val lines = listOf(
            ClinkDiagnostic.GattConnection(133, false) to "gatt_connection status=133 state=disconnected",
            ClinkDiagnostic.GattConnection(0, true) to "gatt_connection status=0 state=connected",
            ClinkDiagnostic.GattServices(5) to "gatt_services status=5",
            ClinkDiagnostic.Mtu(512, 0) to "ble_mtu mtu=512 status=0",
            ClinkDiagnostic.Mtu(null, null) to "ble_mtu result=none",
            ClinkDiagnostic.Characteristics(true, false, null) to "ble_characteristics request=found reply=missing notify=skipped",
            ClinkDiagnostic.Characteristics(false, true, false) to "ble_characteristics request=missing reply=found notify=refused",
            ClinkDiagnostic.Characteristics(true, true, true) to "ble_characteristics request=found reply=found notify=ok",
            ClinkDiagnostic.Cccd(ClinkDiagnostic.CccdResult.FAILED, 3) to "ble_cccd result=failed status=3",
            ClinkDiagnostic.Cccd(ClinkDiagnostic.CccdResult.REFUSED) to "ble_cccd result=refused",
            ClinkDiagnostic.Cccd(ClinkDiagnostic.CccdResult.SUCCESS) to "ble_cccd result=success",
            ClinkDiagnostic.BleWrite(7) to "ble_write status=7",
            ClinkDiagnostic.BleWrite(null) to "ble_write result=refused",
            ClinkDiagnostic.ReplyInvalid("bytes=32 fragments=2 error=missing_ssid") to
                "ble_reply_invalid bytes=32 fragments=2 error=missing_ssid")
        lines.forEach { (event, line) -> assertEquals(line, event.line()) }
        val codes = mapOf(ClinkFailureReason.BLE_DISCOVERY to "ble_discovery_failed",
            ClinkFailureReason.BLE_SERVICE_MISSING to "ble_service_missing",
            ClinkFailureReason.BLE_SUBSCRIBE to "ble_subscribe_failed", ClinkFailureReason.BLE_CCCD to "ble_cccd_failed",
            ClinkFailureReason.BLE_WRITE to "ble_write_failed", ClinkFailureReason.BLE_REPLY_INVALID to "ble_reply_invalid",
            ClinkFailureReason.BLE_VERIFY_TIMEOUT to "ble_verify_timeout", ClinkFailureReason.BLE_EXCHANGE to "ble_exchange_failed")
        codes.forEach { (reason, code) -> assertEquals(code, ClinkFailures.describe(ClinkStage.BLE_EXCHANGE, ClinkFailure(reason)).code) }
    }
    @Test fun invalidReplyDiagnosticRejectsFreeText() {
        for (text in listOf("private password", "bytes=1 error=private-password", "error=secret\nbytes=1", "error=", "")) {
            try { ClinkDiagnostic.ReplyInvalid(text); fail("Accepted free text") }
            catch (_: IllegalArgumentException) { }
        }
    }

    @Test fun publishedPeerAndTrustFactsContainOnlyPresentationValues() {
        assertEquals("car_name My car", ClinkDiagnostic.Peer("My car\n").line())
        assertEquals("car_name " + "a".repeat(80), ClinkDiagnostic.Peer("a".repeat(100)).line())
        assertEquals("car_trusted=true", ClinkDiagnostic.Trusted(true).line())
        assertEquals("car_trusted=false", ClinkDiagnostic.Trusted(false).line())
    }
}
