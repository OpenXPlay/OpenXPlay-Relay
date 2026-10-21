// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-20
package org.openxplay.direlay.publicname

import org.junit.Assert.*
import org.junit.Test
import org.openxplay.direlay.publicname.PublicNamePolicy.DAY
import org.openxplay.direlay.publicname.PublicNamePolicy.HOUR
import org.openxplay.direlay.publicname.PublicNamePolicy.MINUTE
import org.openxplay.direlay.publicname.PublicNamePolicy.OrderDecision
import java.util.Random

class PublicNamePolicyTest {
    @Test fun backoffIsOneFiveThirtyMinutesTwoAndSixHoursThenDaily() {
        assertEquals(listOf(MINUTE, 5 * MINUTE, 30 * MINUTE, 2 * HOUR, 6 * HOUR, DAY, DAY, DAY), (1..8).map(PublicNamePolicy::backoff))
    }

    @Test fun addressPrefersTheHotspotThenTheLowestSortedAndOnlyPrivateOnes() {
        fun choose(vararg candidates: AddressCandidate) = PublicNamePolicy.chooseAddress(candidates.toList())
        assertEquals("192.168.43.1", choose(AddressCandidate("10.0.0.5", false), AddressCandidate("192.168.43.1", true)))
        assertEquals("10.0.0.5", choose(AddressCandidate("192.168.1.9", false), AddressCandidate("10.0.0.5", false)))
        assertEquals("172.16.0.1", choose(AddressCandidate("100.64.0.1", true), AddressCandidate("172.16.0.1", false)))
        assertNull(choose(AddressCandidate("8.8.8.8", true), AddressCandidate("169.254.1.1", false)))
        assertNull(choose())
        for (address in listOf("10.0.0.0", "10.255.255.255", "172.16.0.0", "172.31.255.255", "192.168.0.0", "192.168.255.255"))
            assertTrue(address, PublicNamePolicy.isPrivateIpv4(address))
        for (address in listOf("172.15.0.1", "172.32.0.1", "192.169.0.1", "11.0.0.1", "100.64.0.1", "127.0.0.1", "10.0.0", "10.0.0.256",
            "10.0.0.1.2", "10.0.0.-1", "fe80::1", ""))
            assertFalse(address, PublicNamePolicy.isPrivateIpv4(address))
    }

    @Test fun renewalUsesTheAriTimeWhenOfferedElseTheLastThirdOfTheLifetime() {
        val notBefore = 0L
        val notAfter = 90 * DAY
        assertEquals(60 * DAY, PublicNamePolicy.renewalTime(notBefore, notAfter, null))
        assertFalse(PublicNamePolicy.renewalDue(notBefore, notAfter, null, 60 * DAY - 1))
        assertTrue(PublicNamePolicy.renewalDue(notBefore, notAfter, null, 60 * DAY))
        assertEquals(40 * DAY, PublicNamePolicy.renewalTime(notBefore, notAfter, 40 * DAY))
        assertTrue(PublicNamePolicy.renewalDue(notBefore, notAfter, 40 * DAY, 40 * DAY))
        assertFalse("the ARI window replaces the one-third rule", PublicNamePolicy.renewalDue(notBefore, notAfter, 80 * DAY, 70 * DAY))
    }

    @Test fun ariPicksATimeInsideTheWindowAndKeepsItWhileTheWindowStays() {
        val window = RenewalWindow(50 * DAY, 52 * DAY, null)
        val plan = PublicNamePolicy.ariPlan("id", window, null, 10 * DAY, Random(1))
        assertTrue(checkNotNull(plan.renewAt) in window.start..window.end)
        assertEquals(10 * DAY + PublicNamePolicy.CHECK_INTERVAL, plan.nextPoll)
        val again = PublicNamePolicy.ariPlan("id", window, plan, 11 * DAY, Random(2))
        assertEquals(plan.renewAt, again.renewAt)
        assertEquals("Retry-After is bounded to at least an hour", 11 * DAY + HOUR, PublicNamePolicy.ariPlan("id", window.copy(retryAfterMillis = 1), plan, 11 * DAY, Random(3)).nextPoll)
        val moved = PublicNamePolicy.ariPlan("id", RenewalWindow(20 * DAY, 20 * DAY, 3 * DAY), plan, 12 * DAY, Random(4))
        assertEquals(20 * DAY, moved.renewAt)
        assertEquals(13 * DAY, moved.nextPoll)
    }

    @Test fun localOrderGuard() {
        val now = 100 * DAY
        assertEquals(OrderDecision.Allowed, PublicNamePolicy.orderDecision(emptyList(), null, false, now))
        // A valid certificate with more than 7 days left stays unless renewal is due.
        assertEquals(OrderDecision.KeepCurrent, PublicNamePolicy.orderDecision(emptyList(), now + 8 * DAY, false, now))
        assertEquals(OrderDecision.Allowed, PublicNamePolicy.orderDecision(emptyList(), now + 8 * DAY, true, now))
        assertEquals(OrderDecision.Allowed, PublicNamePolicy.orderDecision(emptyList(), now + 6 * DAY, false, now))
        // At most 3 new orders per name per 24 h; the next one is allowed 24 h after the oldest of them.
        val orders = listOf(now - 23 * HOUR, now - 2 * HOUR, now - HOUR)
        assertEquals(OrderDecision.Wait(now + HOUR), PublicNamePolicy.orderDecision(orders, null, false, now))
        assertEquals(OrderDecision.Allowed, PublicNamePolicy.orderDecision(orders.drop(1) + (now - 25 * HOUR), null, false, now))
    }
}
