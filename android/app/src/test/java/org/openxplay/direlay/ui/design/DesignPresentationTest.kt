// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-24
package org.openxplay.direlay.ui.design

import org.junit.Assert.*
import org.junit.Test
import org.openxplay.direlay.path.*

class DesignPresentationTest {
    @Test fun elapsedTimersClampFutureAndMissingStartTimes() {
        assertEquals(0L, elapsedSeconds(null, 20_000))
        assertEquals(0L, elapsedSeconds(21_000, 20_000))
        assertEquals(0L, elapsedSeconds(20_000, 20_000))
        assertEquals(12L, elapsedSeconds(7001, 20_000))
        assertEquals(13L, elapsedSeconds(7000, 20_000))
        assertEquals("00:00", sessionTime(-1))
        assertEquals("00:59", sessionTime(59))
        assertEquals("01:00", sessionTime(60))
        assertEquals("12:31", sessionTime(751))
        assertEquals("120:00", sessionTime(7200))
    }
    @Test fun titleCollapseClampsAndStartsCollapsedInShortWindows() {
        assertEquals(0f, largeTitleCollapseFraction(-10f, 64f, false), 0f)
        assertEquals(.5f, largeTitleCollapseFraction(32f, 64f, false), 0f)
        assertEquals(1f, largeTitleCollapseFraction(100f, 64f, false), 0f)
        assertEquals(1f, largeTitleCollapseFraction(0f, 64f, true), 0f)
        assertEquals(1f, largeTitleCollapseFraction(0f, 0f, false), 0f)
    }
    @Test fun everyStepHasAnIconHueAndGalleryTitle() {
        StepId.entries.forEach { id ->
            assertTrue(stepIcon(id) > 0)
            assertNotNull(stepHue(id))
            assertTrue(sampleStepTitle(id) > 0)
        }
        assertEquals(StepId.entries.size, StepId.entries.map(::sampleStepTitle).distinct().size)
        assertEquals(stepIcon(StepId.BLUETOOTH), stepIcon(StepId.IPHONE_BLUETOOTH))
        assertEquals(IconHue.IDENTITY, stepHue(StepId.IDENTITY))
        assertEquals(IconHue.NETWORK, stepHue(StepId.HOTSPOT))
        assertEquals(IconHue.PICTURE, stepHue(StepId.PICTURE))
        assertEquals(StepState.entries.size, StepState.entries.map(::stateLabel).distinct().size)
    }
    @Test fun sectionsKeepTheRequiredOrderAndNumbers() {
        assertEquals(listOf("①", "②", "③", "④"), PathSection.entries.map(::sectionNumber))
        assertEquals(4, PathSection.entries.map(::sectionTitle).distinct().size)
    }
}
