// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-20
package org.openxplay.direlay.publicname

import java.util.Random

/** A car-facing IPv4 address and whether it sits on the relay's own hotspot. */
data class AddressCandidate(val address: String, val hotspot: Boolean)

/** Pure scheduling rules, shared wording with the Linux relay: backoff, local order guard, renewal timing. */
object PublicNamePolicy {
    const val MINUTE = 60_000L
    const val HOUR = 60 * MINUTE
    const val DAY = 24 * HOUR
    private val backoff = listOf(MINUTE, 5 * MINUTE, 30 * MINUTE, 2 * HOUR, 6 * HOUR)
    const val CHECK_INTERVAL = 12 * HOUR
    const val REPUBLISH_AFTER = DAY
    const val ORDERS_PER_DAY = 3
    const val KEEP_VALID_FOR = 7 * DAY

    /** 1, 5, 30 min, 2 h, 6 h, then every 24 h; [failures] counts from 1. */
    fun backoff(failures: Int): Long = backoff.getOrElse(failures - 1) { DAY }

    /** The hotspot address when the relay runs one, else the lowest-sorted; only RFC 1918 addresses. */
    fun chooseAddress(candidates: Collection<AddressCandidate>): String? = candidates.filter { isPrivateIpv4(it.address) }
        .sortedWith(compareByDescending<AddressCandidate> { it.hotspot }.thenBy { it.address }).firstOrNull()?.address

    fun isPrivateIpv4(address: String): Boolean {
        val parts = address.split('.')
        if (parts.size != 4 || parts.any { it.isEmpty() || it.length > 3 || !it.all { c -> c in '0'..'9' } }) return false
        val octets = parts.map { it.toInt() }
        if (octets.any { it > 255 }) return false
        return octets[0] == 10 || octets[0] == 172 && octets[1] in 16..31 || octets[0] == 192 && octets[1] == 168
    }

    /** ARI window when the CA offers one, else when less than a third of the lifetime remains. */
    fun renewalDue(notBefore: Long, notAfter: Long, ariRenewAt: Long?, now: Long): Boolean =
        if (ariRenewAt != null) now >= ariRenewAt else now >= notAfter - (notAfter - notBefore) / 3

    /** When renewal becomes due, for the next check. */
    fun renewalTime(notBefore: Long, notAfter: Long, ariRenewAt: Long?): Long = ariRenewAt ?: (notAfter - (notAfter - notBefore) / 3)

    /** A random time inside the suggested window, kept while the window stays the same. */
    fun ariPlan(certId: String, window: RenewalWindow, previous: AriState?, now: Long, random: Random): AriState {
        val nextPoll = now + (window.retryAfterMillis ?: CHECK_INTERVAL).coerceIn(HOUR, DAY)
        if (previous != null && previous.certId == certId && previous.windowStart == window.start &&
            previous.windowEnd == window.end && previous.renewAt != null) return previous.copy(nextPoll = nextPoll)
        val span = (window.end - window.start).coerceAtLeast(0)
        val renewAt = window.start + if (span == 0L) 0 else (random.nextDouble() * span).toLong()
        return AriState(certId, renewAt, window.start, window.end, nextPoll)
    }

    sealed class OrderDecision {
        data object Allowed : OrderDecision()
        /** The valid certificate stays; no request is needed yet. */
        data object KeepCurrent : OrderDecision()
        /** Three orders in the last 24 h: the next is allowed at [at]. */
        data class Wait(val at: Long) : OrderDecision()
    }

    /** At most 3 new orders per name per 24 h, and none while a valid one has more than 7 days left unless renewal is due. */
    fun orderDecision(orders: List<Long>, validUntil: Long?, renewalDue: Boolean, now: Long): OrderDecision {
        if (validUntil != null && validUntil - now > KEEP_VALID_FOR && !renewalDue) return OrderDecision.KeepCurrent
        val recent = orders.filter { now - it < DAY }.sorted()
        if (recent.size >= ORDERS_PER_DAY) return OrderDecision.Wait(recent[recent.size - ORDERS_PER_DAY] + DAY)
        return OrderDecision.Allowed
    }
}
