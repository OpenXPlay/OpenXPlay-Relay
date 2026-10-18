// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-18
package org.openxplay.direlay.relay

import kotlin.math.abs

data class ViewAreaSize(val width: Int, val height: Int)
data class WindowSizeRecord(val screenWidth: Int, val screenHeight: Int, val densityDpi: Int,
    val width: Int, val height: Int, val fullscreen: Boolean, val count: Long, val lastSeen: Long) {
    val headUnit get() = "$screenWidth×$screenHeight@$densityDpi"
}

/** Pure geometry; declarations are frozen for a session, while observations keep accumulating. */
object ViewAreaPlanner {
    fun plan(width: Int, height: Int, records: List<WindowSizeRecord>, current: ViewAreaSize? = null): List<ViewAreaSize> {
        require(width >= 2 && height >= 2 && width % 2 == 0 && height % 2 == 0)
        val areas = mutableListOf(ViewAreaSize(width, height))
        val candidates = records.groupBy { it.width to it.height }.values.map { rows ->
            rows.maxBy { it.lastSeen }.copy(count = rows.sumOf { it.count }) }.sortedWith(
            compareByDescending<WindowSizeRecord> { it.count }.thenByDescending { it.lastSeen })
            .map { ViewAreaSize(it.width, it.height) }.ifEmpty { listOfNotNull(current) }
        for (size in candidates) {
            if (size.width <= 0 || size.height <= 0 || aspectDistance(size.width, size.height, width, height) <= .02) continue
            val scale = minOf(width.toDouble() / size.width, height.toDouble() / size.height)
            val area = ViewAreaSize((size.width * scale / 2).toInt().coerceAtLeast(1) * 2,
                (size.height * scale / 2).toInt().coerceAtLeast(1) * 2)
            if (areas.none { aspectDistance(it.width, it.height, area.width, area.height) <= .02 }) areas.add(area)
            if (areas.size == 4) break
        }
        // Keep the test area only until real observations exist.
        if (records.isEmpty() && areas.size == 1) areas.add(ViewAreaSize((width * 3 / 8).coerceAtLeast(1) * 2, height))
        return areas.toList()
    }
    private fun aspectDistance(w: Int, h: Int, targetW: Int, targetH: Int) = abs((w.toDouble() / h) / (targetW.toDouble() / targetH) - 1)
    fun closest(areas: List<ViewAreaSize>, width: Int, height: Int): Int? {
        if (width <= 0 || height <= 0) return null
        return areas.indices.minByOrNull { aspectDistance(width, height, areas[it].width, areas[it].height) }
            ?.takeIf { aspectDistance(width, height, areas[it].width, areas[it].height) <= .05 }
    }
}

/** Debounce the selected area, without allowing repeated identical reports to postpone it. */
class ViewAreaSwitchDebounce {
    private var candidate: Int? = null
    private var since = 0L
    fun observe(index: Int?, now: Long): Boolean {
        if (index == candidate) return false
        candidate = index; since = now; return true
    }
    fun ready(now: Long): Int? = candidate?.takeIf { now - since >= 300 }
    fun clear() { candidate = null }
}

/** Numeric private persistence, bounded on both read and write. Counts represent observed visits. */
class WindowSizeRecords(private val read: () -> String, private val write: (String) -> Unit) {
    private val rows = ArrayList<WindowSizeRecord>()
    init {
        runCatching { read().lineSequence().take(200).forEach { line ->
            val v = line.split(',').map { it.toLongOrNull() ?: return@forEach }
            if (v.size == 9 && v.take(5).all { it in 1..32768 } && v[5] in 0..1 && v[6] > 0 && v[7] >= 0 && v[8] == 1L)
                rows.add(WindowSizeRecord(v[0].toInt(), v[1].toInt(), v[2].toInt(), v[3].toInt(), v[4].toInt(), v[5] == 1L, v[6], v[7]))
        } }
        trim()
    }
    @Synchronized fun record(screenWidth: Int, screenHeight: Int, densityDpi: Int, width: Int, height: Int,
        fullscreen: Boolean, now: Long) {
        if (listOf(screenWidth, screenHeight, densityDpi, width, height).any { it !in 1..32768 } || now < 0) return
        val previous = rows.firstOrNull { it.screenWidth == screenWidth && it.screenHeight == screenHeight &&
            it.densityDpi == densityDpi && it.width == width && it.height == height && it.fullscreen == fullscreen }
        rows.remove(previous)
        rows.add(WindowSizeRecord(screenWidth, screenHeight, densityDpi, width, height, fullscreen,
            ((previous?.count ?: 0) + 1).coerceAtLeast(1), now))
        trim(); persist()
    }
    @Synchronized fun snapshot(): List<WindowSizeRecord> = rows.sortedByDescending { it.lastSeen }
    @Synchronized fun clear() { rows.clear(); persist() }
    private fun trim() {
        val units = rows.groupBy { it.headUnit }.entries.sortedByDescending { entry -> entry.value.maxOf { it.lastSeen } }.take(10)
        rows.clear()
        units.forEach { rows.addAll(it.value.sortedByDescending { row -> row.lastSeen }.distinctBy { row -> Triple(row.width, row.height, row.fullscreen) }.take(20)) }
    }
    private fun persist() = write(rows.joinToString("\n") {
        "${it.screenWidth},${it.screenHeight},${it.densityDpi},${it.width},${it.height},${if (it.fullscreen) 1 else 0},${it.count},${it.lastSeen},1"
    })
}
