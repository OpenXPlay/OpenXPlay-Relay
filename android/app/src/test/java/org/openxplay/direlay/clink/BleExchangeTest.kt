// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-14
package org.openxplay.direlay.clink

import org.junit.Assert.*
import org.junit.Test
import org.openxplay.stream.clink.ble.*

class BleExchangeTest {
    private class FakeGatt : BleExchange.Gatt {
        val calls = mutableListOf<String>()
        var accepted = true
        var payload: ByteArray? = null
        override fun requestMtu(size: Int): Boolean { calls += "mtu:$size"; return accepted }
        override fun discover(): Boolean { calls += "discover"; return accepted }
        override fun subscribeLocally(): Boolean { calls += "local-subscribe"; return accepted }
        override fun subscribeCccd(): Boolean { calls += "cccd"; return accepted }
        override fun write(payload: ByteArray): Boolean { calls += "write"; this.payload = payload; return accepted }
        var unchanged = true
        override fun requestUnchanged(): Boolean { calls += "verify"; return unchanged }
    }
    /** Mirrors the Android adapter: only a confirmed rejection switches the peer's request type. */
    private class Outcome(val policy: BlePeerPolicy = BlePeerPolicy()) {
        var replies = 0
        val reasons = mutableListOf<BleExchange.Reason>()
        fun complete(value: HeadUnitReply?, error: Throwable?) {
            if (value != null) { replies++; return }
            val reason = (error as BleExchange.Failure).reason
            reasons += reason
            if (reason == BleExchange.Reason.REQUEST_REJECTED) policy.rejected("car", policy.choice("car"))
        }
    }
    private fun submitted(gatt: FakeGatt, outcome: Outcome, acknowledged: Boolean) =
        BleExchange(gatt, phone, complete = outcome::complete).apply {
            connected(); mtu(true); services(true); descriptorWritten(true)
            if (acknowledged) written(true)
        }
    private val phone = BleCodec.phoneRequest(PhoneRequest("relay", "OpenXPlay", "Android", 1, "02:01:02:03:04:05"))
    private val reply = """{"name":"Car","ssid":"DIRECT-Car","psk":"12345678","mac":"aa:bb:cc:dd:ee:ff","freq":2412,"port":57209,"type":1001}""".toByteArray()
    @Test fun exchangeSubscribesBeforeWritingAndAcceptsEarlyIndication() {
        val gatt = FakeGatt()
        var replies = 0
        val exchange = BleExchange(gatt, phone) { value, failure ->
            assertNull(failure); assertEquals("DIRECT-Car", value!!.ssid); replies++
        }
        exchange.connected(); exchange.mtu(true); exchange.services(true)
        assertEquals(listOf("mtu:512", "discover", "local-subscribe", "cccd"), gatt.calls)
        assertNull(gatt.payload)
        exchange.descriptorWritten(true)
        assertEquals(listOf("mtu:512", "discover", "local-subscribe", "cccd", "write"), gatt.calls)
        assertArrayEquals(phone, gatt.payload)
        exchange.indication(reply)
        assertEquals(0, replies)
        exchange.written(true)
        assertEquals(1, replies)
        exchange.indication(reply); exchange.fail()
        assertEquals(1, replies)
    }
    @Test fun refusalMalformedReplyAndDisconnectFinishOnce() {
        for (failure in 0..3) {
            val gatt = FakeGatt()
            var completions = 0
            val exchange = BleExchange(gatt, phone) { value, error ->
                assertNull(value); assertNotNull(error); completions++
            }
            exchange.connected(); exchange.mtu(false)
            if (failure == 0) gatt.accepted = false
            exchange.services(failure != 1); exchange.descriptorWritten(true)
            if (failure == 2) { exchange.written(true); exchange.indication("invalid".toByteArray()) }
            if (failure == 3) exchange.written(false)
            exchange.fail(); exchange.fail()
            assertEquals(1, completions)
        }
    }
    @Test fun fragmentedIndicationsWaitForCompleteJson() {
        var replies = 0
        val exchange = BleExchange(FakeGatt(), phone) { value, failure ->
            assertNull(failure); assertNotNull(value); replies++
        }
        exchange.connected(); exchange.mtu(false); exchange.services(true); exchange.descriptorWritten(true); exchange.written(true)
        for (fragment in reply.toList().chunked(7).dropLast(1)) exchange.indication(fragment.toByteArray())
        assertEquals(0, replies)
        exchange.indication(reply.toList().chunked(7).last().toByteArray())
        assertEquals(1, replies)
    }
    @Test fun cccdTimeoutRequiresFreshLegacyConnection() {
        val policy = BlePeerPolicy()
        val key = policy.key("car", "address")
        val choice = policy.choice(key)
        val first = FakeGatt()
        var completions = 0
        val exchange = BleExchange(first, phone) { _, error ->
            assertEquals(BleExchange.Reason.CCCD_TIMEOUT, (error as BleExchange.Failure).reason)
            policy.cccdTimedOut(key, choice)
            completions++
        }
        exchange.connected(); exchange.mtu(true); exchange.services(true)
        exchange.cccdTimeout(); exchange.descriptorWritten(true); exchange.cccdTimeout()
        assertEquals(1, completions)
        assertNull(first.payload)
        val fresh = FakeGatt()
        val retry = BleExchange(fresh, phone, policy.choice(key).legacy) { _, _ -> }
        retry.connected(); retry.mtu(true); retry.services(true)
        assertEquals(listOf("mtu:512", "discover", "local-subscribe", "write"), fresh.calls)
    }
    @Test fun serviceChangedAfterSubmissionRejectsOnlyOnceRediscoveryConfirmsIt() {
        for (acknowledged in listOf(false, true)) for (gone in listOf(true, false)) {
            val gatt = FakeGatt().apply { unchanged = false }
            val outcome = Outcome()
            val exchange = submitted(gatt, outcome, acknowledged)
            exchange.serviceChanged(); exchange.serviceChanged()
            assertEquals(BleExchange.State.VERIFYING, exchange.state)
            assertTrue(outcome.reasons.isEmpty())
            // An unacknowledged write is drained before the rediscovery.
            assertEquals(if (acknowledged) "discover" else "write", gatt.calls.last())
            if (!acknowledged) exchange.written(true)
            assertEquals(2, gatt.calls.count { it == "discover" })
            // The rediscovery shows no Carlink service, or a request characteristic at another handle.
            if (gone) exchange.serviceRemoved() else exchange.services(true)
            exchange.indication(reply); exchange.written(true); exchange.services(true)
            assertEquals(listOf(BleExchange.Reason.REQUEST_REJECTED), outcome.reasons)
            assertEquals(0, outcome.replies)
            assertEquals(1001, outcome.policy.choice("car").requestType)
        }
    }
    @Test fun republishedDatabaseContinuesTheSameRequest() {
        for (acknowledged in listOf(false, true)) {
            val gatt = FakeGatt()
            val outcome = Outcome()
            val exchange = submitted(gatt, outcome, acknowledged)
            exchange.serviceChanged()
            if (!acknowledged) exchange.written(true)
            exchange.services(true)
            assertEquals(BleExchange.State.REPLY, exchange.state)
            exchange.verifyTimeout()
            exchange.indication(reply)
            assertEquals(1, outcome.replies)
            assertTrue(outcome.reasons.isEmpty())
            assertEquals(1, gatt.calls.count { it == "write" })
            assertEquals(1002, outcome.policy.choice("car").requestType)
        }
    }
    @Test fun unconfirmedServiceChangeFailsWithoutSwitchingTheType() {
        for (case in 0..2) {
            val gatt = FakeGatt()
            val outcome = Outcome()
            val exchange = submitted(gatt, outcome, acknowledged = case != 1)
            exchange.serviceChanged()
            when (case) {
                0 -> exchange.verifyTimeout()
                1 -> { exchange.written(false); exchange.services(true) }
                2 -> exchange.services(false)
            }
            exchange.indication(reply)
            assertEquals(listOf(BleExchange.Reason.FAILED), outcome.reasons)
            assertEquals(1002, outcome.policy.choice("car").requestType)
        }
        // A failed write still counts as a rejection once the service is confirmed gone.
        val outcome = Outcome()
        val exchange = submitted(FakeGatt(), outcome, acknowledged = false)
        exchange.serviceChanged(); exchange.written(false); exchange.serviceRemoved()
        assertEquals(listOf(BleExchange.Reason.REQUEST_REJECTED), outcome.reasons)
    }
    @Test fun replyDuringVerificationEndsItAndStaleDiscoveryIsIgnored() {
        for (acknowledged in listOf(false, true)) {
            val gatt = FakeGatt().apply { unchanged = false }
            val outcome = Outcome()
            val exchange = submitted(gatt, outcome, acknowledged)
            exchange.serviceChanged()
            // Before the drained write, no discovery of ours is pending: a result cannot be judged.
            if (!acknowledged) { exchange.serviceRemoved(); exchange.services(true) }
            assertEquals(BleExchange.State.VERIFYING, exchange.state)
            exchange.indication(reply)
            if (!acknowledged) exchange.written(true)
            exchange.serviceRemoved(); exchange.services(true)
            assertEquals(1, outcome.replies)
            assertTrue(outcome.reasons.isEmpty())
            assertFalse(gatt.calls.contains("verify"))
        }
    }
    @Test fun serviceChangedBeforeSubmissionRediscoversAndResubscribes() {
        for (subscribing in listOf(false, true)) {
            val gatt = FakeGatt()
            val exchange = BleExchange(gatt, phone) { _, _ -> fail("Unexpected completion") }
            exchange.connected(); exchange.mtu(true)
            if (subscribing) exchange.services(true)
            exchange.serviceChanged()
            if (subscribing) {
                assertEquals(BleExchange.State.REDISCOVERING, exchange.state)
                assertEquals("cccd", gatt.calls.last())
                exchange.descriptorWritten(true)
            }
            assertEquals(BleExchange.State.SERVICES, exchange.state)
            assertEquals("discover", gatt.calls.last())
            exchange.descriptorWritten(true); exchange.cccdTimeout()
            assertNull(gatt.payload)
            exchange.services(true); exchange.descriptorWritten(true)
            assertArrayEquals(phone, gatt.payload)
        }
    }
    @Test fun serviceChangeCannotHideAnUnansweredCccd() {
        var reason: BleExchange.Reason? = null
        val gatt = FakeGatt()
        val exchange = BleExchange(gatt, phone) { _, error -> reason = (error as BleExchange.Failure).reason }
        exchange.connected(); exchange.mtu(true); exchange.services(true)
        exchange.serviceChanged(); exchange.cccdTimeout()
        assertEquals(BleExchange.Reason.CCCD_TIMEOUT, reason)
        assertNull(gatt.payload)
    }
    @Test fun descriptorFailureDoesNotWriteAndLateTimeoutCannotRejectReply() {
        val gatt = FakeGatt()
        var failures = 0
        val exchange = BleExchange(gatt, phone) { _, error -> assertNotNull(error); failures++ }
        exchange.connected(); exchange.mtu(true); exchange.services(true); exchange.descriptorWritten(false)
        assertNull(gatt.payload)
        assertEquals(1, failures)
        var replies = 0
        val success = BleExchange(FakeGatt(), phone) { value, error ->
            assertNull(error); assertNotNull(value); replies++
        }
        success.connected(); success.mtu(true); success.services(true); success.descriptorWritten(true)
        success.cccdTimeout(); success.indication(reply); success.serviceChanged(); success.written(true)
        success.serviceChanged()
        assertEquals(1, replies)
    }
    @Test fun failedConnectionCanBeRetriedWithFreshMachine() {
        val gatt = FakeGatt()
        var failures = 0
        BleExchange(gatt, phone) { _, _ -> failures++ }.fail()
        var success = false
        val retry = BleExchange(gatt, phone) { value, _ -> success = value != null }
        retry.connected(); retry.mtu(true); retry.services(true); retry.descriptorWritten(true); retry.written(true); retry.indication(reply)
        assertEquals(1, failures); assertTrue(success)
    }
    @Test fun onlyPostWriteServiceRemovalRejectsTheType() {
        for (submitted in listOf(false, true)) {
            val policy = BlePeerPolicy()
            var reason: BleExchange.Reason? = null
            val exchange = BleExchange(FakeGatt(), phone) { _, error ->
                reason = (error as BleExchange.Failure).reason
                if (reason == BleExchange.Reason.REQUEST_REJECTED) policy.rejected("car", policy.choice("car"))
            }
            exchange.connected(); exchange.mtu(true); exchange.services(true)
            if (submitted) exchange.descriptorWritten(true)
            exchange.serviceRemoved()
            assertEquals(if (submitted) BleExchange.Reason.REQUEST_REJECTED else BleExchange.Reason.FAILED, reason)
            assertEquals(if (submitted) 1001 else 1002, policy.choice("car").requestType)
        }
    }
    @Test fun genericGattFailureDoesNotSwitchTheType() {
        val policy = BlePeerPolicy()
        val exchange = BleExchange(FakeGatt(), phone) { _, error ->
            if ((error as BleExchange.Failure).reason == BleExchange.Reason.REQUEST_REJECTED)
                policy.rejected("car", policy.choice("car"))
        }
        exchange.connected(); exchange.mtu(true); exchange.services(true); exchange.descriptorWritten(true)
        exchange.fail()
        assertEquals(1002, policy.choice("car").requestType)
    }

}
