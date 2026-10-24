// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-24
package org.openxplay.direlay.ui.design

import androidx.compose.animation.core.SnapSpec
import androidx.compose.animation.core.SpringSpec
import androidx.compose.ui.graphics.Color
import org.junit.Assert.*
import org.junit.Test
import org.openxplay.direlay.path.*

class RelayTokensTest {
    @Test fun palettesAndStateContainersUseTheSpecifiedTokens() {
        assertEquals(Color.Black, RelayTokens.darkScheme.background)
        assertEquals(Color(0xFFF2F2F7), RelayTokens.lightScheme.background)
        assertEquals(Color(0xFF54E8A0), RelayTokens.darkStatus.ok)
        assertEquals(Color(0xFF0E9F63), RelayTokens.lightStatus.ok)
        listOf(RelayTokens.darkStatus, RelayTokens.lightStatus).forEach { colors ->
            StepState.entries.forEach { state ->
                assertEquals(.14f, colors.container(state).alpha, .005f)
                assertEquals(colors.color(state).copy(alpha = .14f), colors.container(state))
            }
            assertEquals(colors.working, colors.color(StepState.WAITING))
            assertEquals(colors.working, colors.color(StepState.WORKING))
            assertEquals(colors.action, colors.color(StepState.ACTION))
            assertEquals(colors.failed, colors.color(StepState.FAILED))
            assertEquals(colors.blocked, colors.color(StepState.BLOCKED))
            assertEquals(colors.ok, colors.color(StepState.OK))
            assertEquals(colors.ok, colors.color(PathTone.LIVE))
            assertEquals(colors.ok, colors.color(PathTone.READY))
            assertEquals(colors.working, colors.color(PathTone.PROGRESS))
            assertEquals(colors.action, colors.color(PathTone.ACTION))
            assertEquals(colors.failed, colors.color(PathTone.ERROR))
            assertEquals(colors.blocked, colors.color(PathTone.IDLE))
        }
    }
    @Test fun disabledMotionSnapsAndEnabledMotionUsesOwnSprings() {
        assertTrue(relaySpatial<Float>(false) is SnapSpec)
        assertTrue(relayEffects<Float>(false) is SnapSpec)
        val spatial = relaySpatial<Float>(true) as SpringSpec
        val effects = relayEffects<Float>(true) as SpringSpec
        assertEquals(.8f, spatial.dampingRatio, 0f)
        assertEquals(380f, spatial.stiffness, 0f)
        assertEquals(1f, effects.dampingRatio, 0f)
        assertEquals(1600f, effects.stiffness, 0f)
    }
}
