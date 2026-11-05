// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-11-04
package org.openxplay.direlay.clink

import org.junit.Assert.*
import org.junit.Test

class CarlinkWifiFlowTest {
    private class Driver : CarlinkWifiFlow.Driver<Int> {
        data class Job(val at: Long, val action: () -> Unit, var cancelled: Boolean = false)
        var time = 0L
        val jobs = mutableListOf<Job>()
        lateinit var observer: CarlinkWifiFlow.Observer<Int>
        lateinit var requested: CarlinkWifiFlow.Observer<Int>
        lateinit var unavailable: () -> Unit
        var requests = 0
        var releases = 0
        var unobserved = 0
        var nudge = 0L
        var nudges = 0
        override fun nudgePlatformJoin(): Long { nudges++; return nudge }
        override fun observe(observer: CarlinkWifiFlow.Observer<Int>): AutoCloseable {
            this.observer = observer
            return AutoCloseable { unobserved++ }
        }
        override fun request(observer: CarlinkWifiFlow.Observer<Int>, unavailable: () -> Unit,
            timeoutMillis: Int): AutoCloseable {
            assertEquals(45_000, timeoutMillis)
            requests++; requested = observer; this.unavailable = unavailable
            return AutoCloseable { releases++ }
        }
        override fun later(millis: Long, action: () -> Unit): AutoCloseable {
            val job = Job(time + millis, action); jobs.add(job)
            return AutoCloseable { job.cancelled = true }
        }
        fun advance(millis: Long) {
            val until = time + millis
            while (true) {
                val job = jobs.filter { !it.cancelled && it.at <= until }.minByOrNull { it.at } ?: break
                job.cancelled = true; time = job.at; job.action()
            }
            time = until
        }
    }
    private fun candidate(id: Int = 1, ssid: String? = "car", encrypted: Boolean? = true,
        peer: String? = "192.168.49.1", local: String? = "192.168.49.2") =
        CarlinkWifiCandidate(id, ssid, encrypted, peer, local)
    private class Fixture(nudge: Long = 0) {
        val driver = Driver().also { it.nudge = nudge }
        val phases = mutableListOf<CarlinkWifiFlow.Phase>()
        var losses = 0
        val flow = CarlinkWifiFlow("car", driver, phases::add, { _, _ -> }, { losses++ })
        init { flow.start() }
        fun automatic() { driver.advance(250) }
    }
    @Test fun reusesVerifiedExistingNetworkWithoutOwningItsAssociation() {
        val f = Fixture()
        f.driver.observer.changed(candidate())
        f.automatic()
        assertEquals(1, f.flow.result.get().network)
        assertEquals(0, f.driver.requests)
        f.flow.close()
        assertEquals(0, f.driver.releases)
        assertEquals(1, f.driver.unobserved)
    }
    @Test fun unrelatedOpenRedactedAndAddresslessNetworksAreNeverAdopted() {
        for (candidate in listOf(candidate(ssid = "other"), candidate(ssid = null),
            candidate(encrypted = false), candidate(peer = null), candidate(local = "0.0.0.0"))) {
            val f = Fixture(); f.driver.observer.changed(candidate); f.automatic()
            assertFalse(f.flow.result.isDone)
            assertEquals(1, f.driver.requests)
            f.flow.close()
        }
    }
    @Test fun ambiguousSameSsidNetworksRequireTheExactTargetRequest() {
        val f = Fixture()
        f.driver.observer.changed(candidate(1)); f.driver.observer.changed(candidate(2))
        f.automatic()
        assertEquals(1, f.driver.requests)
        assertFalse(f.flow.result.isDone)
    }
    @Test fun connectionCanCompleteAfterTheOldTwelveSecondDeadline() {
        val f = Fixture(); f.automatic(); f.driver.advance(20_000)
        f.driver.requested.changed(candidate(ssid = null))
        assertEquals(1, f.flow.result.get().network)
        assertEquals(CarlinkWifiFlow.Phase.READY, f.flow.phase)
        f.driver.advance(60_000)
        assertEquals(0, f.driver.releases)
        f.flow.close(); assertEquals(1, f.driver.releases)
    }
    @Test fun expiryKeepsTheAttemptAliveForManualSwitchAndAdoptsTheNewSystemNetwork() {
        val f = Fixture(); f.driver.observer.changed(candidate(ssid = "home")); f.automatic()
        f.driver.advance(45_000)
        assertEquals(CarlinkWifiFlow.Phase.MANUAL, f.flow.phase)
        assertEquals(1, f.driver.releases)
        assertFalse(f.flow.result.isDone)
        f.driver.advance(120_000)
        f.driver.observer.changed(candidate(2))
        assertEquals(2, f.flow.result.get().network)
        f.flow.close(); assertEquals(1, f.driver.releases)
    }
    @Test fun observerCannotCancelTheRequestThatOwnsItsNewConnection() {
        val f = Fixture(); f.automatic()
        f.driver.observer.changed(candidate())
        assertFalse(f.flow.result.isDone)
        f.driver.requested.changed(candidate())
        assertTrue(f.flow.result.isDone)
        assertEquals(0, f.driver.releases)
    }
    @Test fun staleCallbacksAfterFailureOrStopCannotSelectOrLoseANewConnection() {
        val f = Fixture(); f.automatic()
        val stale = f.driver.requested
        f.driver.unavailable()
        f.driver.observer.changed(candidate(2))
        stale.changed(candidate(3)); stale.lost(2)
        assertEquals(2, f.flow.result.get().network); assertEquals(0, f.losses)
        f.flow.close(); f.driver.observer.lost(2)
        assertEquals(0, f.losses)
    }
    @Test fun aNewRedactedManualNetworkMustPassAnotherExactTargetRequest() {
        val f = Fixture(); f.automatic(); f.driver.unavailable()
        f.driver.observer.changed(candidate(2, ssid = null))
        assertEquals(2, f.driver.requests)
        assertFalse(f.flow.result.isDone)
        f.driver.requested.changed(candidate(2, ssid = null))
        assertEquals(2, f.flow.result.get().network)
    }
    @Test fun mismatchedRequestedNetworkCannotProceedToCarlinkAuthentication() {
        val f = Fixture(); f.automatic()
        f.driver.requested.changed(candidate(ssid = "other"))
        assertEquals(CarlinkWifiFlow.Phase.MANUAL, f.flow.phase)
        assertFalse(f.flow.result.isDone)
    }
    @Test fun returningFromWifiSettingsRefreshesARedactedOrPreviouslyRequestedNetwork() {
        for (ssid in listOf("car", null)) {
            val f = Fixture(); f.automatic()
            val stale = f.driver.observer
            stale.changed(candidate(ssid = null))
            f.driver.unavailable()
            f.flow.refresh()
            stale.changed(candidate(2)); stale.lost(1)
            assertFalse(f.flow.result.isDone)
            f.driver.observer.changed(candidate(ssid = ssid))
            if (ssid == null) {
                assertEquals(2, f.driver.requests)
                assertFalse(f.flow.result.isDone)
                f.driver.requested.changed(candidate(ssid = null))
            } else assertEquals(1, f.driver.requests)
            assertEquals(1, f.flow.result.get().network)
            assertEquals(1, f.driver.unobserved)
            assertEquals(0, f.losses)
            f.flow.close()
        }
    }
    @Test fun refreshDoesNotInterruptAnAutomaticOrEstablishedConnection() {
        val f = Fixture(); f.automatic(); f.flow.refresh()
        assertEquals(0, f.driver.unobserved)
        f.driver.requested.changed(candidate())
        f.flow.refresh()
        assertEquals(0, f.driver.unobserved)
        assertEquals(0, f.driver.releases)
        f.flow.close()
    }
    @Test fun selectedNetworkLossAndAddressChangesAreReportedOnlyOnce() {
        val f = Fixture(); f.driver.observer.changed(candidate()); f.automatic()
        f.driver.observer.changed(candidate(local = "192.168.49.3"))
        f.driver.observer.lost(1)
        assertEquals(1, f.losses)
    }
    @Test fun stoppingWhileWaitingCancelsAllOwnedWork() {
        val f = Fixture(); f.automatic(); f.flow.close(); f.driver.advance(90_000)
        assertTrue(f.flow.result.isCancelled)
        assertEquals(CarlinkWifiFlow.Phase.CLOSED, f.flow.phase)
        assertEquals(1, f.driver.releases); assertEquals(1, f.driver.unobserved)
    }
    @Test fun thePlatformsOwnJoinAfterTheNudgeIsAdoptedWithoutARequest() {
        val f = Fixture(nudge = 3_000); f.automatic()
        assertEquals(1, f.driver.nudges)
        assertEquals(0, f.driver.requests)
        f.driver.advance(400)
        f.driver.observer.changed(candidate())
        assertEquals(1, f.flow.result.get().network)
        assertEquals(0, f.driver.requests)
        f.driver.advance(10_000)
        assertEquals(0, f.driver.requests)
        assertFalse(CarlinkWifiFlow.Phase.AUTOMATIC in f.phases)
    }
    @Test fun withoutAJoinTheNudgeWindowEndsInTheExactTargetRequest() {
        val f = Fixture(nudge = 3_000)
        f.driver.advance(2_999); assertEquals(0, f.driver.requests)
        f.driver.advance(1); assertEquals(1, f.driver.requests)
        f.driver.observer.changed(candidate(ssid = "other"))
        assertFalse(f.flow.result.isDone)
    }
    @Test fun aTargetTheRequestNeverClaimsIsBorrowedFromThePlatform() {
        val f = Fixture(); f.automatic()
        f.driver.observer.changed(candidate())
        f.driver.advance(CarlinkWifiFlow.CLAIM_MILLIS - 1)
        assertFalse(f.flow.result.isDone)
        f.driver.advance(1)
        assertEquals(1, f.flow.result.get().network)
        assertEquals(1, f.driver.releases)
        assertFalse(CarlinkWifiFlow.Phase.MANUAL in f.phases)
        f.driver.requested.changed(candidate(2))
        assertEquals(1, f.flow.result.get().network)
    }
    @Test fun twoUnclaimedTargetsAreNeverBorrowed() {
        val f = Fixture(); f.automatic()
        f.driver.observer.changed(candidate(1)); f.driver.observer.changed(candidate(2))
        f.driver.advance(CarlinkWifiFlow.CLAIM_MILLIS)
        assertFalse(f.flow.result.isDone)
        assertEquals(0, f.driver.releases)
    }
    @Test fun afterTheRequestEndsTheTargetThePlatformJoinedIsAdopted() {
        val f = Fixture(); f.automatic()
        f.driver.observer.changed(candidate(2))
        f.driver.unavailable()
        assertEquals(CarlinkWifiFlow.Phase.MANUAL, f.flow.phase)
        f.driver.advance(CarlinkWifiFlow.CLAIM_MILLIS)
        assertEquals(2, f.flow.result.get().network)
    }
    @Test fun theRequestsOwnNetworkGoingAwayIsNotAdoptedAfterTheRequestEnds() {
        val f = Fixture(); f.automatic()
        f.driver.observer.changed(candidate(2))
        f.driver.unavailable()
        f.driver.observer.lost(2)
        f.driver.advance(CarlinkWifiFlow.CLAIM_MILLIS)
        assertFalse(f.flow.result.isDone)
        assertEquals(CarlinkWifiFlow.Phase.MANUAL, f.flow.phase)
    }
}
