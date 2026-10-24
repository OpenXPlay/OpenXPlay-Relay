// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-24
package org.openxplay.direlay.ui.design

import androidx.compose.animation.EnterTransition
import androidx.compose.animation.ExitTransition
import org.junit.Assert.*
import org.junit.Test
import org.openxplay.direlay.ui.relayPageTransition

class PageMotionTest {
    @Test fun pageMotionIsAbsentWhenDisabledInBothDirections() {
        listOf(true, false).forEach { forward ->
            val transition = relayPageTransition(forward, false)
            assertEquals(EnterTransition.None, transition.targetContentEnter)
            assertEquals(ExitTransition.None, transition.initialContentExit)
            val animated = relayPageTransition(forward, true)
            assertNotEquals(EnterTransition.None, animated.targetContentEnter)
            assertNotEquals(ExitTransition.None, animated.initialContentExit)
        }
    }
}
