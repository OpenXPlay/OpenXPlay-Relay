// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-24
package org.openxplay.direlay.relay

import org.junit.Assert.*
import org.junit.Test
import org.openxplay.direlay.network.NetworkReadings
import org.openxplay.direlay.path.LinkFailure
import org.openxplay.direlay.path.LinkStep

class RecoverOncePolicyTest {
    private fun failure(code: String, retrying: Boolean = false) = LinkFailure(LinkStep.STARTING, code, "failure", 123, retrying)
    private val off = NetworkReadings(wifi = false, hotspot = false, bluetooth = false)
    private val on = NetworkReadings(wifi = true, hotspot = true, bluetooth = true)
    @Test fun eachStoppedRadioFailureRecoversOnceAndExplicitStartResetsTheBudget() {
        for (code in listOf("hotspot_not_ready", "hotspot_configuration", "bluetooth_off", "bluetooth_unusable", "wifi_off")) {
            val policy = RecoverOncePolicy()
            assertNull(policy.observe(failure(code), off, false))
            assertEquals(code, policy.observe(failure(code), on, false))
            assertNull(policy.observe(failure(code), off, false))
            assertNull(policy.observe(failure(code), on, false))
            policy.reset()
            policy.observe(failure(code), off, false)
            assertEquals(code, policy.observe(failure(code), on, false))
        }
    }
    @Test fun onlyTheFailedRadioCanRecoverAndUnknownIsNotAnOffEdge() {
        for ((code, unrelated) in listOf("hotspot_not_ready" to off.copy(wifi = true, bluetooth = true),
            "bluetooth_off" to off.copy(wifi = true, hotspot = true), "wifi_off" to off.copy(hotspot = true, bluetooth = true))) {
            val policy = RecoverOncePolicy()
            policy.observe(failure(code), off, false)
            assertNull(policy.observe(failure(code), unrelated, false))
            assertEquals(code, policy.observe(failure(code), on, false))
        }
        val policy = RecoverOncePolicy()
        assertNull(policy.observe(failure("wifi_off"), on, false))
        policy.observe(failure("wifi_off"), NetworkReadings(), false)
        assertNull(policy.observe(failure("wifi_off"), on, false))
    }
    @Test fun userStopRetryingMissingAndUnrelatedFailuresNeverRestart() {
        for (error in listOf(null, failure("unknown"), failure("first_tcp_timeout"), failure("wifi_off", true))) {
            val policy = RecoverOncePolicy()
            policy.observe(error, off, false)
            assertNull(policy.observe(error, on, false))
        }
        val policy = RecoverOncePolicy()
        policy.observe(failure("bluetooth_off"), off, true)
        assertNull(policy.observe(failure("bluetooth_off"), on, true))
    }
}
