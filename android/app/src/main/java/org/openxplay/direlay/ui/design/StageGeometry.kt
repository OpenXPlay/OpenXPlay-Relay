// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-30
package org.openxplay.direlay.ui.design

import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp

enum class StageLayout(internal val outerTile: Dp, internal val relayTile: Dp,
    internal val outerIcon: Dp, internal val relayIcon: Dp, internal val gap: Dp, internal val height: Dp) {
    VERTICAL(64.dp, 72.dp, 34.dp, 38.dp, 36.dp, 272.dp),
    VERTICAL_COMPACT(56.dp, 64.dp, 28.dp, 32.dp, 24.dp, 224.dp),
    HORIZONTAL(56.dp, 64.dp, 28.dp, 32.dp, 0.dp, 112.dp),
}

internal fun stageLayout(pageHeight: Dp, stageHeight: Dp): StageLayout = when {
    pageHeight < 480.dp -> if (stageHeight >= 232.dp) StageLayout.VERTICAL_COMPACT else StageLayout.HORIZONTAL
    pageHeight >= 700.dp -> StageLayout.VERTICAL
    else -> StageLayout.VERTICAL_COMPACT
}

internal data class StageCurve(val start: Offset, val control1: Offset, val control2: Offset, val end: Offset) {
    fun point(t: Float): Offset {
        val u = 1f - t
        return start * (u * u * u) + control1 * (3f * u * u * t) +
            control2 * (3f * u * t * t) + end * (t * t * t)
    }

    fun tangent(t: Float): Offset {
        val u = 1f - t
        return (control1 - start) * (3f * u * u) + (control2 - control1) * (6f * u * t) +
            (end - control2) * (3f * t * t)
    }
}

internal data class StageNumberAnchor(val point: Offset, val normal: Offset) {
    fun center(textHeight: Float): Offset = point + normal * (textHeight / 2f)
}

internal data class StageCableGeometry(val curve: StageCurve, val number: StageNumberAnchor)
internal data class StageGeometry(val iphone: Rect, val relay: Rect, val car: Rect,
    val iphoneCable: StageCableGeometry, val carCable: StageCableGeometry)

internal fun stageGeometry(size: Size, layout: StageLayout, density: Float): StageGeometry {
    require(layout != StageLayout.HORIZONTAL && density > 0f)
    val width = size.width.coerceAtMost(360f * density)
    val left = (size.width - width) / 2f
    val top = (size.height - layout.height.value * density) / 2f
    val outer = layout.outerTile.value * density
    val middle = layout.relayTile.value * density
    val gap = layout.gap.value * density
    fun tile(x: Float, y: Float, side: Float) = Rect(x - side / 2f, y, x + side / 2f, y + side)
    val iphone = tile(left + width * .25f, top, outer)
    val relay = tile(left + width * .75f, iphone.bottom + gap, middle)
    val car = tile(iphone.center.x, relay.bottom + gap, outer)
    fun cable(far: Rect, upper: Boolean): StageCableGeometry {
        val start = Offset(relay.center.x, if (upper) relay.top - 6f * density else relay.bottom + 6f * density)
        val end = Offset(far.right + 4f * density, far.center.y)
        val curve = StageCurve(start, Offset(start.x, start.y + .55f * (end.y - start.y)),
            Offset(end.x + .55f * (start.x - end.x), end.y), end)
        return StageCableGeometry(curve, stageNumberAnchor(curve, (relay.center + far.center) / 2f, density))
    }
    return StageGeometry(iphone, relay, car, cable(iphone, true), cable(car, false))
}

private fun stageNumberAnchor(curve: StageCurve, tilesMidpoint: Offset, density: Float): StageNumberAnchor {
    // Arc-length sampling keeps geometry independent of Android's PathMeasure and within a fraction of a dp.
    val samples = 64
    val lengths = FloatArray(samples + 1)
    var previous = curve.start
    for (i in 1..samples) {
        val point = curve.point(i.toFloat() / samples)
        lengths[i] = lengths[i - 1] + (point - previous).getDistance()
        previous = point
    }
    val half = lengths.last() / 2f
    val index = lengths.indexOfFirst { it >= half }.coerceAtLeast(1)
    val span = lengths[index] - lengths[index - 1]
    val t = (index - 1 + if (span > 0f) (half - lengths[index - 1]) / span else 0f) / samples
    val midpoint = curve.point(t)
    val tangent = curve.tangent(t)
    val length = tangent.getDistance()
    var normal = if (length > 0f) Offset(-tangent.y, tangent.x) / length else Offset(0f, -1f)
    val away = midpoint - tilesMidpoint
    if (normal.x * away.x + normal.y * away.y < 0f) normal *= -1f
    return StageNumberAnchor(midpoint + normal * (14f * density), normal)
}
