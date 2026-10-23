// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-23
package org.openxplay.direlay.clink

import org.junit.Assert.*
import org.junit.Test
import org.openxplay.direlay.settings.ExperimentalOptions

class BleScanPolicyTest {
    @Test fun ordinaryScanStillTimesOutAfterEightSecondsWithoutRenewal() {
        val policy = BleScanPolicy.forAttempt(false)
        assertEquals(8L, policy.timeoutSeconds)
        assertNull(policy.restartMs)
        val disabled = ExperimentalOptions(keepScanning = true).effective()
        assertEquals(policy, BleScanPolicy.forAttempt(disabled.keepScanning))
    }

    @Test fun continuousScanHasNoEmptyResultTimeoutAndRenewsEveryTenMinutes() {
        val policy = BleScanPolicy.forAttempt(true)
        assertNull(policy.timeoutSeconds)
        assertEquals(600_000L, policy.restartMs)
    }
}
