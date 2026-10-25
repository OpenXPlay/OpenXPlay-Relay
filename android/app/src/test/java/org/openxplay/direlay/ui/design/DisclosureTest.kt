// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-25
package org.openxplay.direlay.ui.design

import androidx.compose.animation.EnterTransition
import androidx.compose.animation.ExitTransition
import androidx.compose.animation.core.SnapSpec
import org.junit.Assert.*
import org.junit.Test

class DisclosureTest {
    @Test fun enabledMotionHasEnterAndExitTransitions() {
        assertNotEquals(EnterTransition.None, relayDisclosureEnter(true))
        assertNotEquals(ExitTransition.None, relayDisclosureExit(true))
    }
    @Test fun disabledMotionSnapsBothSizeAndFadeWithoutDelay() {
        listOf(relayDisclosureEnter(false), relayDisclosureExit(false)).forEach { transition ->
            val data = field(transition, "data")
            listOf("changeSize", "fade").forEach { effect ->
                val spec = field(field(data, effect), "animationSpec")
                assertTrue(spec is SnapSpec<*>)
                assertEquals(0, (spec as SnapSpec<*>).delay)
            }
        }
    }
    // Compose keeps transition specs internal; inspect them without a Compose UI runtime.
    private fun field(owner: Any, name: String): Any = owner.javaClass.getDeclaredField(name).run {
        isAccessible = true
        requireNotNull(get(owner))
    }
}
