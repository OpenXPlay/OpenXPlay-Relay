// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-21
package org.openxplay.direlay.relay

import org.junit.Assert.*
import org.junit.Test
import org.openxplay.stream.core.*

class RelayStatusFailureTest {
    @Test fun keepsTenNewestInputAndOutputFailures() {
        val status = RelayStatus(wallClock = { 0L })
        assertNull(status.failure)
        repeat(12) { index ->
            if (index % 2 == 0) status.inputFailed(IllegalStateException("failure $index"))
            else status.outputFailed("web", IllegalStateException("failure $index"))
        }
        assertEquals((11 downTo 2).map { index ->
            val output = if (index % 2 == 0) null else "web"
            RelayFailure(output, "${output ?: "Input"}: failure $index")
        }, status.failures)
        assertEquals("web: failure 11", status.failure)
    }

    @Test fun identicalTextsMergeAndBecomeNewestWithoutChangingPreviousSnapshots() {
        val status = RelayStatus(wallClock = { 0L })
        status.inputFailed(IllegalStateException("connection closed"))
        val previous = status.failures
        status.outputFailed("web", IllegalStateException("listener failed"))
        status.inputFailed(IllegalArgumentException("connection closed"))
        assertEquals(listOf("Input: connection closed", "web: listener failed"), status.failures.map { it.text })
        assertEquals(listOf(RelayFailure(null, "Input: connection closed")), previous)
        status.inputFailed(IllegalStateException("connection closed", IllegalStateException("auth_rejected")))
        assertEquals(3, status.failures.size)
        assertEquals("Input: connection closed\nCaused by: auth_rejected", status.failure)
    }

    @Test fun clearsOnExplicitResetAndNewSessionButNotSameSessionPhaseChangesOrClose() {
        val status = RelayStatus(wallClock = { 0L })
        fun session(id: Long, phase: SessionPhase) =
            status.onSession(SessionState(SessionId(id), phase), SessionConfig(emptyMap()))
        session(1, SessionPhase.CONNECTING)
        status.inputFailed(IllegalStateException("failed"))
        for (phase in listOf(SessionPhase.ACTIVE, SessionPhase.RECONNECTING, SessionPhase.ENDED)) {
            session(1, phase)
            assertEquals("Input: failed", status.failure)
        }
        session(2, SessionPhase.CONNECTING)
        assertTrue(status.failures.isEmpty())
        assertNull(status.failure)
        status.outputFailed("clink", IllegalStateException("auth_rejected"))
        status.clearFailure()
        assertTrue(status.failures.isEmpty())
        assertNull(status.failure)
        status.inputFailed(IllegalStateException("still failed"))
        status.close()
        assertEquals("Input: still failed", status.failure)
    }

    @Test fun redactsBeforeRetainingMessagesAndKeepsEmptyMessageClassesAndCauses() {
        val status = RelayStatus(redactFailure = { redactFailureText(it, listOf("test-secret")) })
        status.inputFailed(IllegalStateException("", IllegalArgumentException("remote failure: test-secret")))
        assertEquals("Input: IllegalStateException\nCaused by: remote failure: [redacted]", status.failure)
    }
    @Test fun recordsWallTimeAndMergedFailureKeepsNewestTime() {
        var now = 123L
        val status = RelayStatus(wallClock = { now })
        status.inputFailed(IllegalStateException("failed"))
        val previous = status.failures
        now = 456L
        status.inputFailed(IllegalArgumentException("failed"))
        assertEquals(1, status.failures.size)
        assertEquals(456L, status.failures.single().atMillis)
        assertEquals(123L, previous.single().atMillis)
    }

}
