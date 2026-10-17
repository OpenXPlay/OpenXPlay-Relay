// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-16
package org.openxplay.direlay.clink

import org.junit.Assert.*
import org.junit.Test
import java.util.concurrent.CompletableFuture
import java.util.concurrent.TimeoutException

class LinkReconnectTest {
    private class Clock {
        var time = 0L
        val scheduled = sortedMapOf<Long, () -> Unit>()
        fun wait(done: CompletableFuture<Unit>, timeout: Long) {
            val deadline = time + timeout
            while (!done.isDone && scheduled.isNotEmpty() && scheduled.firstKey() <= deadline) {
                time = scheduled.firstKey()
                scheduled.remove(time)!!.invoke()
            }
            if (!done.isDone) { time = deadline; throw TimeoutException() }
            done.get()
        }
    }
    private class Resource(private val name: String, private val calls: MutableList<String>) {
        val closed = CompletableFuture<Unit>()
        fun close() { calls.add("$name:closed"); closed.complete(Unit) }
        fun force() { calls.add("$name:force"); close() }
    }
    @Test fun nextHandoffWaitsForBothGattAndNetworkAcrossLinkInstances() {
        val clock = Clock()
        val calls = mutableListOf<String>()
        val gate = LinkReconnect({ clock.time }, clock::wait)
        val gatt = Resource("gatt", calls)
        val network = Resource("network", calls)
        gate.releasing(gatt.closed, gatt::force)
        gate.releasing(network.closed, network::force)
        clock.scheduled[100] = gatt::close
        clock.scheduled[800] = network::close
        gate.awaitReleased(ClinkDiagnostics.NONE)
        calls.add("new-handoff")
        assertEquals(listOf("gatt:closed", "network:closed", "new-handoff"), calls)
        assertEquals(800L, clock.time)
        gate.awaitReleased(ClinkDiagnostics.NONE)
        assertEquals(800L, clock.time)
    }
    @Test fun timeoutUsesOneFiveSecondBudgetAndForcesRemainingResourcesBeforeRetry() {
        val clock = Clock()
        val calls = mutableListOf<String>()
        val events = mutableListOf<ClinkDiagnostic>()
        val gate = LinkReconnect({ clock.time }, clock::wait)
        val gatt = Resource("gatt", calls)
        val network = Resource("network", calls)
        gate.releasing(gatt.closed, gatt::force)
        gate.releasing(network.closed, network::force)
        gate.awaitReleased(ClinkDiagnostics { events.add(it) })
        calls.add("scan")
        assertEquals(5_000L, clock.time)
        assertEquals(listOf("gatt:force", "gatt:closed", "network:force", "network:closed", "scan"), calls)
        assertEquals(2, events.count { it == ClinkStage.CLEANUP_FORCE })
    }
    @Test fun failedForceCloseFailsOnlyTheCurrentAttempt() {
        val clock = Clock()
        val events = mutableListOf<ClinkDiagnostic>()
        val gate = LinkReconnect({ clock.time }, clock::wait)
        gate.releasing(CompletableFuture()) { error("close_failed") }
        try { gate.awaitReleased(ClinkDiagnostics { events.add(it) }); fail("Must not reconnect") }
        catch (expected: IllegalStateException) { assertEquals("close_failed", expected.message) }
        assertEquals(ClinkDiagnostic.Failure(ClinkStage.CLEANUP_FORCE, "unknown:IllegalStateException",
            "IllegalStateException"), events.last())
        val calls = mutableListOf<String>()
        val gatt = Resource("gatt", calls)
        gate.releasing(gatt.closed, gatt::force)
        clock.scheduled[5_100] = gatt::close
        gate.awaitReleased(ClinkDiagnostics.NONE)
        assertEquals(listOf("gatt:closed"), calls)
        assertEquals(5_100L, clock.time)
    }
    private class FailingGatt {
        val closed = CompletableFuture<Unit>()
        fun closeFailed() { closed.completeExceptionally(java.io.IOException("ssid=private psk=private")) }
    }
    @Test fun exceptionalReleaseIsLoggedDroppedAndNextHandoffProceeds() {
        val clock = Clock()
        val calls = mutableListOf<String>()
        val events = mutableListOf<ClinkDiagnostic>()
        val gate = LinkReconnect({ clock.time }, clock::wait)
        val gatt = FailingGatt()
        val network = Resource("network", calls)
        gate.releasing(gatt.closed) { fail("A finished release must not be forced") }
        gate.releasing(network.closed, network::force)
        clock.scheduled[100] = gatt::closeFailed
        clock.scheduled[200] = network::close
        gate.awaitReleased(ClinkDiagnostics { events.add(it) })
        assertEquals(200L, clock.time)
        assertEquals(listOf(ClinkDiagnostic.Failure(ClinkStage.CLEANUP_WAIT, "unknown:IOException", "IOException")),
            events.filterIsInstance<ClinkDiagnostic.Failure>())
        assertFalse(events.joinToString { it.line() }.contains("private"))
        // The next handoff never waits on, or fails for, the earlier failure.
        val next = Resource("next", calls)
        gate.releasing(next.closed, next::force)
        clock.scheduled[300] = next::close
        events.clear()
        gate.awaitReleased(ClinkDiagnostics { events.add(it) })
        assertEquals(300L, clock.time)
        assertEquals(listOf("network:closed", "next:closed"), calls)
        assertEquals(listOf(ClinkStage.CLEANUP_WAIT, ClinkStage.CLEANUP_DONE), events)
        // A release that failed before anyone waited is dropped without a timeout.
        gate.releasing(FailingGatt().apply { closeFailed() }.closed) { fail("Must not force") }
        gate.awaitReleased(ClinkDiagnostics.NONE)
        events.clear()
        gate.awaitReleased(ClinkDiagnostics { events.add(it) })
        assertEquals(300L, clock.time)
        assertTrue(events.isEmpty())
    }
    @Test fun failedNetworkReleaseStillSettlesTheBarrier() {
        val clock = Clock()
        val calls = mutableListOf<String>()
        val failures = mutableListOf<Throwable>()
        val events = mutableListOf<ClinkDiagnostic>()
        val gate = LinkReconnect({ clock.time }, clock::wait)
        val gatt = Resource("gatt", calls)
        val released = CompletableFuture<Unit>()
        // Mirrors disconnectRun: the network unbind check throws, the other steps still run, and the
        // barrier settles once the GATT close completes.
        gate.releasing(released) { fail("A settled release must not be forced") }
        gate.releasing(gatt.closed, gatt::force)
        settleRelease(released, listOf({ calls.add("gatt:disconnect") }, { error("Network unbind failed") },
            { calls.add("p2p:left") }), failures::add) { gatt.closed }
        assertFalse(released.isDone)
        clock.scheduled[100] = gatt::close
        gate.awaitReleased(ClinkDiagnostics { events.add(it) })
        assertEquals(100L, clock.time)
        assertEquals(listOf("gatt:disconnect", "p2p:left", "gatt:closed"), calls)
        assertEquals(listOf("Network unbind failed"), failures.map { it.message })
        assertTrue(released.isCompletedExceptionally)
        assertEquals(1, events.count { it is ClinkDiagnostic.Failure && it.stage == ClinkStage.CLEANUP_WAIT })
        gate.awaitReleased(ClinkDiagnostics.NONE)
        assertEquals(100L, clock.time)
    }
    @Test fun forcedReleaseSettlesEvenWhenNetworkReleaseThrows() {
        val clock = Clock()
        val calls = mutableListOf<String>()
        val events = mutableListOf<ClinkDiagnostic>()
        val gate = LinkReconnect({ clock.time }, clock::wait)
        val released = CompletableFuture<Unit>()
        // The release posted by disconnectRun never ran; forcing it must still settle the barrier.
        gate.releasing(released) {
            settleRelease(released, listOf({ calls.add("gatt:force") }, { error("Network unbind failed") }), {})
        }
        gate.awaitReleased(ClinkDiagnostics { events.add(it) })
        assertEquals(5_000L, clock.time)
        assertEquals(listOf("gatt:force"), calls)
        assertTrue(released.isCompletedExceptionally)
        assertEquals(listOf(ClinkStage.CLEANUP_WAIT, ClinkStage.CLEANUP_FORCE,
            ClinkDiagnostic.Failure(ClinkStage.CLEANUP_WAIT, "unknown:IllegalStateException", "IllegalStateException"),
            ClinkStage.CLEANUP_DONE), events)
        gate.awaitReleased(ClinkDiagnostics.NONE)
        assertEquals(5_000L, clock.time)
    }
    @Test fun backoffCapsAtFifteenSecondsAndSuccessResetsIt() {
        val backoff = LinkBackoff()
        assertEquals(listOf(1_000L, 2_000L, 4_000L, 8_000L, 15_000L, 15_000L), List(6) { backoff.next() })
        backoff.succeeded()
        assertEquals(1_000L, backoff.next())
    }
}
