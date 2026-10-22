// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-15
package org.openxplay.direlay.clink

import org.junit.Assert.*
import org.junit.Test
import java.util.concurrent.Executors
import java.util.concurrent.LinkedBlockingQueue
import java.util.concurrent.TimeUnit

class DialogPinRetryTest {
    @Test fun rejectedCodeReopensAnEditableSheetAndRetryIsConsumedOnce() {
        val provider = DialogPinProvider()
        val requests = LinkedBlockingQueue<DialogPinProvider.Request>()
        provider.onPending = { provider.pending?.let(requests::add) }
        val executor = Executors.newSingleThreadExecutor()
        try {
            val answer = executor.submit<String?> { provider.requestPin("car") }
            val first = requireNotNull(requests.poll(3, TimeUnit.SECONDS))
            assertFalse(first.incorrect); first.respond("111111")
            assertEquals("111111", answer.get(3, TimeUnit.SECONDS)); assertNull(provider.pending)
            assertEquals("auth_rejected [redacted]", provider.redactFailureMessage("auth_rejected 111111"))
            var reconnects = 0
            provider.reject { reconnects++ }
            val retry = requireNotNull(requests.poll(3, TimeUnit.SECONDS))
            assertTrue(retry.incorrect); assertEquals(1, provider.rejected)
            retry.respond("222222"); retry.respond("333333")
            assertEquals(1, reconnects)
            assertEquals("[redacted] [redacted]", provider.redactFailureMessage("111111 222222"))
            provider.retire(); provider.resumeRequests()
            assertEquals("222222", provider.requestPin("car"))
            provider.paired(); assertEquals(1, provider.successes)
            val next = executor.submit<String?> { provider.requestPin("car") }
            val newRequest = requireNotNull(requests.poll(3, TimeUnit.SECONDS))
            assertFalse(newRequest.incorrect)
            provider.cancel(); assertNull(next.get(3, TimeUnit.SECONDS))
        } finally { provider.cancel(); executor.shutdownNow() }
    }
    @Test fun cancellingASheetInvokesTheRuntimeCancellationAndUnblocksTheRequest() {
        val provider = DialogPinProvider()
        val requests = LinkedBlockingQueue<DialogPinProvider.Request>()
        var cancelled = 0
        provider.onCancel = { cancelled++ }
        provider.onPending = { provider.pending?.let(requests::add) }
        val executor = Executors.newSingleThreadExecutor()
        try {
            val answer = executor.submit<String?> { provider.requestPin("car") }
            requireNotNull(requests.poll(3, TimeUnit.SECONDS)).cancel()
            assertNull(answer.get(3, TimeUnit.SECONDS)); assertNull(provider.pending)
            assertEquals(1, cancelled)
        } finally { provider.cancel(); executor.shutdownNow() }
    }
}
