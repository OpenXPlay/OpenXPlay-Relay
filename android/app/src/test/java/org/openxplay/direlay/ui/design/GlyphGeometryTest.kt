// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-24
package org.openxplay.direlay.ui.design

import org.junit.Assert.*
import org.junit.Test
import org.openxplay.direlay.path.StepState

class GlyphGeometryTest {
    @Test fun everyContourHas96FinitePositiveSamples() {
        StepState.entries.forEach { state ->
            val radii = glyphRadii(state)
            assertEquals(96, radii.size)
            assertTrue(radii.all { it.isFinite() && it > 0f && it <= 1.2f })
        }
    }
    @Test fun circlesAndRingAreConstantWhileScallopHasEightLobes() {
        assertEquals(1, glyphRadii(StepState.WAITING).distinct().size)
        assertEquals(1, glyphRadii(StepState.BLOCKED).distinct().size)
        val work = glyphRadii(StepState.WORKING)
        val peaks = work.indices.count { work[it] > work[(it + 95) % 96] && work[it] > work[(it + 1) % 96] }
        assertEquals(8, peaks)
        val star = glyphRadii(StepState.FAILED)
        assertEquals(5, star.indices.count { star[it] > star[(it + 95) % 96] && star[it] > star[(it + 1) % 96] })
        assertTrue(glyphRadii(StepState.BLOCKED)[0] < glyphRadii(StepState.WAITING)[0])
    }
    @Test fun interpolationPreservesEndpointsAndSupportsSpringOvershoot() {
        val from = glyphRadii(StepState.BLOCKED)
        val to = glyphRadii(StepState.FAILED)
        val result = FloatArray(96)
        interpolateRadii(from, to, 0f, result)
        assertArrayEquals(from, result, .00001f)
        interpolateRadii(from, to, 1f, result)
        assertArrayEquals(to, result, .00001f)
        interpolateRadii(from, to, .5f, result)
        result.indices.forEach { assertEquals((from[it] + to[it]) / 2, result[it], .00001f) }
        interpolateRadii(to, from, 1.5f, result)
        assertTrue(result.all { it.isFinite() && it > 0f })
    }
    @Test(expected = IllegalArgumentException::class) fun interpolationRejectsDifferentSampleCounts() {
        interpolateRadii(FloatArray(96), FloatArray(95), 0f, FloatArray(96))
    }
}
