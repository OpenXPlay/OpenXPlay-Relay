// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-14
package org.openxplay.direlay.clink

import org.junit.Assert.*
import org.junit.Test
import java.util.concurrent.Executors
import java.util.concurrent.TimeUnit

class PinSheetStateTest {
    @Test fun numericFieldValidatesSixDigitsAndRespondsOnlyOnce() {
        val replies = mutableListOf<String?>()
        val state = PinSheetState(false, replies::add)
        state.edit("12a３4")
        assertEquals("124", state.digits)
        assertFalse(state.canSubmit)
        state.submit()
        assertTrue(replies.isEmpty())
        state.edit("012345678")
        assertEquals("012345", state.digits)
        state.submit()
        state.submit(); state.cancel(); state.edit("999999")
        assertEquals(listOf("012345"), replies)
        assertTrue(state.finished)
        assertEquals("", state.digits)
    }
    @Test fun dismissAndConfirmationEachHaveOneTerminalAnswer() {
        val replies = mutableListOf<String?>()
        PinSheetState(false, replies::add).apply { edit("123456"); cancel(); submit(); cancel() }
        PinSheetState(true, replies::add).apply { assertTrue(canSubmit); submit(); cancel() }
        assertEquals(listOf(null, "approved"), replies)
    }
    @Test fun providerRejectsOverlappingRequestsAndCancellationUnblocksWaiter() {
        val provider = DialogPinProvider()
        val notifications = java.util.concurrent.atomic.AtomicInteger()
        provider.onPending = { notifications.incrementAndGet() }
        val executor = Executors.newSingleThreadExecutor()
        try {
            val first = executor.submit<String?> { provider.requestPin("car") }
            val deadline = System.nanoTime() + TimeUnit.SECONDS.toNanos(3)
            while (provider.pending == null && System.nanoTime() < deadline) Thread.yield()
            val request = requireNotNull(provider.pending)
            assertNull(provider.requestPin("other"))
            assertSame(request, provider.pending)
            PinSheetState(false, request::respond).apply { edit("012345"); submit() }
            assertEquals("012345", first.get(3, TimeUnit.SECONDS))
            assertNull(provider.pending)
            assertEquals(1, notifications.get())
            provider.cancel()
            assertNull(provider.requestPin("car"))
            provider.resumeRequests()
            val confirmation = executor.submit<Boolean> { provider.confirmPairing("car") }
            val secondDeadline = System.nanoTime() + TimeUnit.SECONDS.toNanos(3)
            while (provider.pending == null && System.nanoTime() < secondDeadline) Thread.yield()
            assertTrue(requireNotNull(provider.pending).confirmation)
            provider.cancel()
            assertFalse(confirmation.get(3, TimeUnit.SECONDS))
            assertEquals(2, notifications.get())
            assertNull(provider.pending)
        } finally { provider.cancel(); executor.shutdownNow() }
    }
}
