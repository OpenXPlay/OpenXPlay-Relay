// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-21
package org.openxplay.direlay.clink

import org.junit.Assert.*
import org.junit.Test

class CarlinkResizePolicyTest {
    private val small = CarlinkResizePolicy.Size(1532, 1013)
    private val full = CarlinkResizePolicy.Size(1814, 1200)
    private fun policy() = CarlinkResizePolicy().apply { enabled = true }
    private fun CarlinkResizePolicy.window(size: CarlinkResizePolicy.Size) = windowSize(size.width, size.height)
    private fun CarlinkResizePolicy.view(size: CarlinkResizePolicy.Size) = viewSize(size.width, size.height)
    private fun CarlinkResizePolicy.session(size: CarlinkResizePolicy.Size) {
        startSession()
        window(size)
    }

    @Test fun twoPixelsOnEitherSideMatchButThreePixelsOnEitherSideReconnect() {
        for (dx in -2..2) for (dy in -2..2) {
            val policy = policy()
            policy.session(small)
            policy.viewSize(small.width + dx, small.height + dy)
            assertNull("offset=$dx,$dy", policy.reconnect(playing = true))
        }
        for ((dx, dy) in listOf(3 to 0, -3 to 0, 0 to 3, 0 to -3)) {
            val policy = policy()
            policy.session(small)
            val view = CarlinkResizePolicy.Size(small.width + dx, small.height + dy)
            policy.view(view)
            assertEquals(CarlinkResizePolicy.Resize(small, view), policy.reconnect(playing = true))
        }
    }

    @Test fun disabledOrNotPlayingDoesNotConsumeTheSessionsTrigger() {
        val policy = CarlinkResizePolicy()
        policy.session(small)
        policy.view(full)
        assertFalse(policy.enabled)
        assertNull(policy.reconnect(playing = true))
        policy.enabled = true
        assertNull(policy.reconnect(playing = false))
        assertEquals(CarlinkResizePolicy.Resize(small, full), policy.reconnect(playing = true))
    }

    @Test fun bothSizesMustBeKnownAndANewSessionForgetsTheOldViewAndWindow() {
        val policy = policy()
        policy.view(full)
        assertNull(policy.reconnect(playing = true))
        policy.window(small)
        assertNotNull(policy.reconnect(playing = true))
        policy.startSession()
        assertNull(policy.reconnect(playing = true))
        policy.window(full)
        assertNull(policy.reconnect(playing = true))
        policy.view(small)
        assertEquals(CarlinkResizePolicy.Resize(full, small), policy.reconnect(playing = true))
    }

    @Test fun latestViewIsUsedAndEachSessionTriggersAtMostOnce() {
        val policy = policy()
        policy.session(small)
        policy.view(full)
        policy.view(small)
        assertNull(policy.reconnect(playing = true))
        policy.view(full)
        assertNotNull(policy.reconnect(playing = true))
        assertNull(policy.reconnect(playing = true))
        policy.viewSize(1920, 1080)
        assertNull(policy.reconnect(playing = true))
        policy.enabled = false
        policy.enabled = true
        assertNull(policy.reconnect(playing = true))
    }

    @Test fun unchangedWindowMarksOnlyThatPairUnhelpfulForAllLaterSessions() {
        val policy = policy()
        policy.session(small)
        policy.view(full)
        assertNotNull(policy.reconnect(playing = true))
        // A two-pixel difference is still an unchanged window after reconnecting.
        policy.session(CarlinkResizePolicy.Size(small.width + 2, small.height - 2))
        policy.view(full)
        assertNull(policy.reconnect(playing = true))
        repeat(3) {
            policy.session(small)
            policy.viewSize(full.width - 2, full.height + 2)
            assertNull(policy.reconnect(playing = true))
        }
        policy.viewSize(1920, 1080)
        assertNotNull(policy.reconnect(playing = true))
    }

    @Test fun helpfulReconnectClearsPendingAndTheSamePairCanTriggerAgain() {
        val policy = policy()
        policy.session(small)
        policy.view(full)
        assertNotNull(policy.reconnect(playing = true))
        // Repeated capabilities in the triggering session must not judge its own reconnect.
        policy.window(small)
        policy.session(full)
        policy.view(full)
        assertNull(policy.reconnect(playing = true))
        // If pending had survived the helpful result, returning to the old window would blacklist this pair.
        policy.session(small)
        policy.view(full)
        assertEquals(CarlinkResizePolicy.Resize(small, full), policy.reconnect(playing = true))
    }

    @Test fun helpfulWindowChangesReconnectOnEverySwitchInBothDirections() {
        val policy = policy()
        for ((window, view) in listOf(small to full, full to small, small to full, full to small)) {
            policy.session(window)
            policy.view(window)
            assertNull(policy.reconnect(playing = true))
            policy.view(view)
            assertEquals(CarlinkResizePolicy.Resize(window, view), policy.reconnect(playing = true))
        }
    }

    @Test fun turningOffClearsPendingBeforeTheNextWindowArrives() {
        val policy = policy()
        policy.session(small)
        policy.view(full)
        assertNotNull(policy.reconnect(playing = true))
        policy.enabled = false
        policy.session(small)
        policy.view(full)
        assertNull(policy.reconnect(playing = true))
        policy.enabled = true
        assertNotNull(policy.reconnect(playing = true))
    }

    @Test fun turningOffKeepsPreviouslyUnhelpfulPairs() {
        val policy = policy()
        policy.session(small)
        policy.view(full)
        assertNotNull(policy.reconnect(playing = true))
        policy.session(small)
        policy.enabled = false
        policy.startSession()
        policy.enabled = true
        policy.window(small)
        policy.view(full)
        assertNull(policy.reconnect(playing = true))
    }

    @Test fun missingAndNonPositiveSizesAreIgnored() {
        val policy = policy()
        for ((width, height) in listOf(0 to 1200, 1814 to 0, -1 to 1200, 1814 to -1)) {
            policy.windowSize(width, height)
            policy.viewSize(width, height)
        }
        assertNull(policy.reconnect(playing = true))
        policy.window(small)
        assertNull(policy.reconnect(playing = true))
        policy.view(full)
        policy.viewSize(0, 0)
        policy.windowSize(-1, -1)
        assertEquals(CarlinkResizePolicy.Resize(small, full), policy.reconnect(playing = true))
    }
}
