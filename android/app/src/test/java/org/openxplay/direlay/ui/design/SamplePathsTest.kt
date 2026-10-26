// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-24
package org.openxplay.direlay.ui.design

import org.junit.Assert.*
import org.junit.Test
import org.openxplay.direlay.R
import org.openxplay.direlay.path.*
import org.openxplay.direlay.ui.RelayBackStack
import org.openxplay.direlay.ui.relayRouteParents

class SamplePathsTest {
    @Test fun allNineScenariosHaveConsistentSectionsAndFocus() {
        val samples = samplePaths(1_000_000)
        assertEquals(9, samples.size)
        assertEquals(9, samples.map { it.title }.distinct().size)
        samples.forEach { (_, path) ->
            assertEquals(PathSection.entries, path.sections.map { it.section })
            assertEquals(path.steps.size, path.steps.map { it.id }.distinct().size)
            path.sections.forEach { section -> assertTrue(section.steps.all { it.id.section == section.section }) }
            path.focus?.let { assertTrue(it in path.steps); assertNotEquals(StepState.OK, it.state) }
            assertTrue(path.diagnostic.startsWith("R MODE"))
        }
        val paths = samples.associate { it.title to it.path }
        assertFalse(paths.getValue(R.string.design_sample_idle).canStart)
        assertTrue(paths.getValue(R.string.design_sample_ready).canStart)
        assertTrue(paths.getValue(R.string.design_sample_armed).running)
        assertEquals(R.string.design_armed_detail, paths.getValue(R.string.design_sample_armed).steps.first { it.id == StepId.CARPLAY_START }.detail?.res)
        assertEquals(R.string.design_tap_connect, paths.getValue(R.string.design_sample_ready).steps.first { it.id == StepId.CARPLAY_START }.detail?.res)
        assertEquals(StepId.HOTSPOT, paths.getValue(R.string.design_sample_armed).focus?.id)
        assertEquals(StepAction.START_NOW, paths.getValue(R.string.design_sample_car_wait).focus?.action)
        assertEquals(StepState.WORKING, paths.getValue(R.string.design_sample_connecting).focus?.state)
        assertEquals("first_tcp_timeout", paths.getValue(R.string.design_sample_failed).focus?.code)
        assertEquals(StepAction.RETRY, paths.getValue(R.string.design_sample_failed).focus?.action)
        assertEquals(StepState.WORKING, paths.getValue(R.string.design_sample_retrying).focus?.state)
        assertFalse(paths.getValue(R.string.design_sample_retrying).diagnostic.contains(" | E "))
        assertEquals(StepAction.ENTER_PIN, paths.getValue(R.string.design_sample_pin).focus?.action)
        val live = paths.getValue(R.string.design_sample_live)
        assertNull(live.focus)
        assertEquals(249_000L, live.live?.sinceMillis)
        assertEquals(PathTone.LIVE, live.tone)
        assertTrue(live.steps.all { it.state == StepState.OK })
        assertEquals("1 页", samplePaths(1_000_000, "1 页").last().path.live?.chips?.last())
    }
    @Test fun galleryHasDeveloperParentAndReturnsToTheHub() {
        assertEquals(listOf("settings", "developer"), relayRouteParents("developer/design"))
        val stack = RelayBackStack().open("developer/design")
        assertEquals(listOf("home", "settings", "developer", "developer/design"), stack.entries)
        assertEquals("developer", stack.back().current)
    }
}
