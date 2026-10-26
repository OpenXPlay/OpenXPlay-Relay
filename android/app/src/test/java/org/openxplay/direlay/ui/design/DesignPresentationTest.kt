// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-24
package org.openxplay.direlay.ui.design

import org.junit.Assert.*
import org.junit.Test
import org.openxplay.direlay.path.*

class DesignPresentationTest {
    @Test fun evidenceUsesProportionalTextForCjkAndFullwidthCharacters() {
        listOf("", "OpenXPlay-1A2B · 5 GHz", "https://192.168.43.1:8443", "1920×720 · 60 fps", "Café").forEach {
            assertTrue(it, evidenceUsesMonospace(it))
        }
        listOf("已安装", "手机的热点 · 5 GHz", "1 页", "。", "，", "Ａ", "カ").forEach {
            assertFalse(it, evidenceUsesMonospace(it))
        }
        listOf('\u2E80', '\u9FFF', '\u3000', '\u303F', '\uF900', '\uFAFF', '\uFF00', '\uFFEF').forEach {
            assertFalse(it.toString(), evidenceUsesMonospace("name${it}value"))
        }
        listOf('\u2E7F', '\uA000', '\uF8FF', '\uFB00', '\uFEFF', '\uFFF0').forEach {
            assertTrue(it.toString(), evidenceUsesMonospace(it.toString()))
        }
    }
    @Test fun automaticSectionsFoldOnlyOkStepsAndKeepEveryFocusCandidateInOrder() {
        val states = listOf(StepState.OK, StepState.FAILED, StepState.OK, StepState.ACTION,
            StepState.WORKING, StepState.WAITING, StepState.BLOCKED)
        val ids = listOf(StepId.MODE, StepId.IDENTITY, StepId.PERMISSIONS, StepId.BLUETOOTH,
            StepId.WIFI, StepId.HOTSPOT, StepId.SHARED_WIFI)
        val steps = ids.zip(states) { id, state -> PathStep(id, state, sampleStepTitle(id)) }
        val section = PathSectionView(PathSection.RELAY, steps)
        assertEquals(listOf(StepId.IDENTITY, StepId.BLUETOOTH, StepId.WIFI, StepId.HOTSPOT, StepId.SHARED_WIFI),
            visibleSectionSteps(section, false).map { it.id })
        assertEquals(steps, visibleSectionSteps(section, true))
        val blocked = section.copy(steps = steps.map { it.copy(state = if (it.state == StepState.OK) StepState.OK else StepState.BLOCKED) })
        assertEquals(StepState.BLOCKED, blocked.state)
        assertEquals(steps.filter { it.state != StepState.OK }.map { it.id }, visibleSectionSteps(blocked, false).map { it.id })
        assertEquals(blocked.steps, visibleSectionSteps(blocked, true))
        val allOk = section.copy(steps = steps.map { it.copy(state = StepState.OK) })
        assertEquals(allOk.steps, visibleSectionSteps(allOk, true))
        assertEquals(allOk.steps, visibleSectionSteps(allOk, false))
        assertTrue(visibleSectionSteps(PathSectionView(PathSection.CAR, emptyList()), false).isEmpty())
    }
    @Test fun diagnosticDisplayKeepsCodesWithGlyphsAndSectionLettersWithFirstTokens() {
        val source = "R MODE✓ ID~ PERM! BT✗ HS· | C NET· WEB✓ PAGE~ | E hotspot_configuration 14:31:58 | 0.1.6 14:32:05"
        assertEquals(listOf("R\u00A0MODE\u2060✓ ID\u2060~ PERM\u2060! BT\u2060✗ HS\u2060·",
            "C\u00A0NET\u2060· WEB\u2060✓ PAGE\u2060~", "E\u00A0hotspot_configuration 14:31:58", "0.1.6 14:32:05"),
            diagnosticSections(source))
        assertEquals(listOf("I\u00A0PAIR\u2060✓ CP\u2060·", "P\u00A0LIVE\u2060·"), diagnosticSections("I PAIR✓ CP· | P LIVE·"))
        assertEquals(listOf(""), diagnosticSections(""))
    }
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
