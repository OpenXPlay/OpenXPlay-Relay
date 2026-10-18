// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-18
package org.openxplay.direlay.relay

import org.junit.jupiter.api.Assertions.*
import org.junit.jupiter.api.Test

class ViewAreasTest {
    private fun row(width: Int, height: Int, count: Long = 1, seen: Long = 1, full: Boolean = false) =
        WindowSizeRecord(1920, 1080, 160, width, height, full, count, seen)
    private fun sizes(areas: List<ViewAreaSize>) = areas.map { ViewAreaSize(it.width, it.height) }
    @Test fun fitsEvenSizesAtOriginWithoutChangingAspect() {
        val areas = ViewAreaPlanner.plan(1920, 1080, listOf(row(701, 903), row(5001, 1201)))
        assertEquals(ViewAreaSize(1920, 1080), areas.first())
        for ((area, source) in areas.drop(1).zip(listOf(701.0 / 903, 5001.0 / 1201))) {
            assertTrue(area.width in 2..1920 && area.height in 2..1080)
            assertEquals(0, area.width % 2); assertEquals(0, area.height % 2)
            assertEquals(source, area.width.toDouble() / area.height, .01)
        }
    }
    @Test fun ranksByFrequencyThenRecencyDropsNearDuplicatesAndLimitsToThreeWindows() {
        val areas = ViewAreaPlanner.plan(1920, 1080, listOf(row(1000, 500, 9), row(1001, 501, 8),
            row(600, 600, 7, 2), row(400, 600, 7, 1), row(200, 600, 6), row(1920, 1080, 100, full = true)))
        assertEquals(listOf(ViewAreaSize(1920, 1080), ViewAreaSize(1920, 960),
            ViewAreaSize(1080, 1080), ViewAreaSize(720, 1080)), sizes(areas))
    }
    @Test fun fallbackOnlyWhenThereAreNoRecordedWindowsAndFullscreenDeduplicates() {
        assertEquals(listOf(ViewAreaSize(1280, 720), ViewAreaSize(720, 720)),
            sizes(ViewAreaPlanner.plan(1280, 720, emptyList(), ViewAreaSize(400, 400))))
        assertEquals(listOf(ViewAreaSize(1280, 720)),
            ViewAreaPlanner.plan(1280, 720, listOf(row(1920, 1080, full = true)), ViewAreaSize(400, 400)))
        assertEquals(listOf(ViewAreaSize(1280, 720)), sizes(ViewAreaPlanner.plan(1280, 720, emptyList())))
        assertEquals(listOf(ViewAreaSize(1280, 720)), sizes(ViewAreaPlanner.plan(1280, 720, emptyList(), ViewAreaSize(1280, 720))))
        assertEquals(listOf(ViewAreaSize(1280, 720)), sizes(ViewAreaPlanner.plan(1280, 720, emptyList(), ViewAreaSize(1290, 720))))
        assertEquals(listOf(ViewAreaSize(1280, 720)), ViewAreaPlanner.plan(1280, 720, listOf(row(640, 360))))
        assertEquals(listOf(ViewAreaSize(1920, 960)), sizes(ViewAreaPlanner.plan(1920, 960, emptyList())))
        assertEquals(ViewAreaSize(720, 720), sizes(ViewAreaPlanner.plan(1280, 720,
            listOf(row(500, 500)), ViewAreaSize(300, 600)))[1])
    }
    @Test fun usesRealFullscreenSizesWithoutMixingVisitCountsAndMatchesOnlyCloseAspects() {
        val areas = ViewAreaPlanner.plan(1920, 960, listOf(row(1533, 1014, 6, full = true),
            row(1533, 1014, 6), row(856, 1014, 10, full = true), row(1900, 960, 50)))
        assertEquals(3, areas.size)
        assertEquals(1533.0 / 1014, areas[2].width.toDouble() / areas[2].height, .003)
        assertEquals(2, ViewAreaPlanner.closest(areas, 1533, 1014))
        assertEquals(0, ViewAreaPlanner.closest(areas, 2000, 1000))
        assertNull(ViewAreaPlanner.closest(areas, 1200, 1000))
        assertNull(ViewAreaPlanner.closest(areas, 0, 1000))
        assertNull(ViewAreaPlanner.closest(emptyList(), 1000, 1000))
    }
    @Test fun separateLibrariesRankFitDeduplicateAndLimitIndependently() {
        val records = listOf(row(1500, 1000, 20, full = true), row(1510, 1000, 19, full = true),
            row(1000, 1000, 18, full = true), row(500, 1000, 17, full = true), row(300, 1000, 16, full = true),
            row(1500, 1000, 40), row(1200, 1000, 30), row(800, 1000, 20), row(400, 1000, 10),
            row(2000, 1000, 100, full = true))
        val areas = ViewAreaPlanner.plan(1920, 960, records)
        assertEquals(ViewAreaSize(1920, 960), areas[0])
        assertEquals(6, areas.size)
        assertEquals(ViewAreaLibrary.entries.toSet(), areas[1].libraries)
        for (library in ViewAreaLibrary.entries) assertEquals(3, areas.count { library in it.libraries })
        assertEquals(listOf(1.5, 1.0, .5, 1.2, .8), areas.drop(1).map { it.width.toDouble() / it.height })
        areas.forEach { assertTrue(it.width in 2..1920 && it.height in 2..960); assertEquals(0, it.width % 2); assertEquals(0, it.height % 2) }
    }
    @Test fun preferenceGatesMatchingAndKeepsCurrentDuringReverseCameraWindows() {
        val areas = ViewAreaPlanner.plan(1920, 960, listOf(row(1500, 1000, full = true), row(1000, 1000)))
        assertEquals(1, ViewAreaPlanner.match(areas, 1500, 1000, true, true))
        assertNull(ViewAreaPlanner.match(areas, 1000, 1000, false, true))
        assertNull(ViewAreaPlanner.match(areas, 1500, 1000, true, false))
        assertNull(ViewAreaPlanner.match(areas, 1500, 1000, false, false))
        assertEquals(2, ViewAreaPlanner.match(areas, 1000, 1000, false, false))
        assertEquals(0, ViewAreaPlanner.match(areas, 2000, 1000, false, false))
        assertNull(ViewAreaPlanner.match(areas, 1000, 1000, null, false))
        assertNull(ViewAreaPlanner.match(areas, 1250, 1000, true, true))
        for (full in listOf(true, false, null)) {
            assertEquals(1, ViewAreaPlanner.match(areas, 1500, 1000, full, null))
            assertEquals(2, ViewAreaPlanner.match(areas, 1000, 1000, full, null))
        }

    }
    @Test fun libraryFallbackBelongsToTheObservedModeAndRealRecordsSuppressTestArea() {
        val areas = ViewAreaPlanner.plan(1920, 960, emptyList(), ViewAreaSize(1000, 1000), false)
        assertEquals(setOf(ViewAreaLibrary.WINDOWED), areas[1].libraries)
        assertEquals(1, ViewAreaPlanner.plan(1920, 960, listOf(row(2000, 1000))).size)
        assertEquals(1, ViewAreaPlanner.plan(1920, 960, emptyList()).size)
    }
    @Test fun switchingDebouncesStableTargetsAndCancelsUnmatchedOrDisabledObservations() {
        val debounce = ViewAreaSwitchDebounce()
        assertTrue(debounce.observe(1, 0)); assertNull(debounce.ready(299))
        assertFalse(debounce.observe(1, 200)); assertEquals(1, debounce.ready(300))
        assertTrue(debounce.observe(2, 310)); assertNull(debounce.ready(609)); assertEquals(2, debounce.ready(610))
        assertTrue(debounce.observe(null, 620)); assertNull(debounce.ready(1000))
        debounce.observe(1, 1100); debounce.clear(); assertNull(debounce.ready(1500))
    }
    @Test fun storeCountsFlagsAndDeviceRatiosSeparatelyAndPersistsClear() {
        var disk = ""
        fun store() = WindowSizeRecords({ disk }, { disk = it })
        val records = store()
        records.record(1920, 1080, 160, 1000, 700, false, 1)
        records.record(1920, 1080, 160, 1000, 700, false, 2)
        records.record(1920, 1080, 160, 1000, 700, true, 3)
        records.record(1920, 1080, 320, 1000, 700, false, 4)
        val restored = store().snapshot()
        assertEquals(records.snapshot(), restored)
        assertEquals(3, restored.size)
        assertEquals(2L, restored.last().count)
        assertEquals(2L, restored.last().lastSeen)
        records.clear(); assertTrue(store().snapshot().isEmpty())
    }
    @Test fun storeEvictsOldestSizesAndHeadUnitsAndRejectsInvalidReports() {
        var disk = ""
        val records = WindowSizeRecords({ disk }, { disk = it })
        for (unit in 1..11) for (size in 1..21)
            records.record(1000 + unit, 800, 160, 500 + size, 400, false, unit * 100L + size)
        val rows = records.snapshot()
        assertEquals(200, rows.size)
        assertEquals(10, rows.map { it.headUnit }.distinct().size)
        assertTrue(rows.none { it.screenWidth == 1001 || it.width == 501 })
        records.record(0, 0, 0, 0, 0, false, 0)
        assertEquals(rows, records.snapshot())
        assertEquals(rows, WindowSizeRecords({ disk }, {}).snapshot())
        assertTrue(WindowSizeRecords({ "bad data\n1,2,3" }, {}).snapshot().isEmpty())
    }
}
