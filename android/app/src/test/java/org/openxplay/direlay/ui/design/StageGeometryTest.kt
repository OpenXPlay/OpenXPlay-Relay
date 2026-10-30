// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-30
package org.openxplay.direlay.ui.design

import androidx.compose.ui.geometry.Size
import androidx.compose.ui.unit.dp
import org.junit.Assert.*
import org.junit.Test
import kotlin.math.abs

class StageGeometryTest {
    @Test fun layoutUsesPageHeightAndAvailableLandscapeStageHeight() {
        listOf(
            Triple(700, 100, StageLayout.VERTICAL),
            Triple(699, 500, StageLayout.VERTICAL_COMPACT),
            Triple(480, 100, StageLayout.VERTICAL_COMPACT),
            Triple(479, 232, StageLayout.VERTICAL_COMPACT),
            Triple(340, 232, StageLayout.VERTICAL_COMPACT),
            Triple(340, 231, StageLayout.HORIZONTAL),
            Triple(200, 80, StageLayout.HORIZONTAL),
        ).forEach { (page, stage, expected) -> assertEquals(expected, stageLayout(page.dp, stage.dp)) }
    }

    @Test fun tilesStayInsideTheCenteredStageWithTheSpecifiedSizesAndGaps() {
        listOf(StageLayout.VERTICAL, StageLayout.VERTICAL_COMPACT).forEach { layout ->
            val outer = if (layout == StageLayout.VERTICAL) 64f else 56f
            val middle = if (layout == StageLayout.VERTICAL) 72f else 64f
            val gap = if (layout == StageLayout.VERTICAL) 36f else 24f
            val height = if (layout == StageLayout.VERTICAL) 272f else 224f
            listOf(1f, 2.75f).forEach { density -> listOf(224f, 312f, 360f, 600f).forEach { width ->
                val size = Size(width * density, (height + 20f) * density)
                val stage = stageGeometry(size, layout, density)
                val stageWidth = width.coerceAtMost(360f) * density
                val left = (size.width - stageWidth) / 2f
                listOf(stage.iphone, stage.relay, stage.car).forEach { tile ->
                    assertTrue(tile.left >= 0f && tile.top >= 0f && tile.right <= size.width && tile.bottom <= size.height)
                }
                assertEquals(left + stageWidth * .25f, stage.iphone.center.x, .001f)
                assertEquals(left + stageWidth * .75f, stage.relay.center.x, .001f)
                assertEquals(stage.iphone.center.x, stage.car.center.x, .001f)
                assertEquals(outer * density, stage.iphone.width, .001f)
                assertEquals(outer * density, stage.car.height, .001f)
                assertEquals(middle * density, stage.relay.width, .001f)
                assertEquals(gap * density, stage.relay.top - stage.iphone.bottom, .001f)
                assertEquals(gap * density, stage.car.top - stage.relay.bottom, .001f)
                assertEquals(height * density, stage.car.bottom - stage.iphone.top, .001f)
                assertEquals(10f * density, stage.iphone.top, .001f)
            } }
        }
    }

    @Test fun elbowsClearTheRingAndMeetTheOuterTilesWithTheSpecifiedTangents() {
        listOf(StageLayout.VERTICAL, StageLayout.VERTICAL_COMPACT).forEach { layout ->
            listOf(1f, 2.75f).forEach { density ->
                val stage = stageGeometry(Size(312f * density, layout.height.value * density), layout, density)
                listOf(stage.iphoneCable to stage.iphone, stage.carCable to stage.car).forEach { (cable, far) ->
                    val curve = cable.curve
                    val upper = far == stage.iphone
                    assertEquals(stage.relay.center.x, curve.start.x, .001f)
                    assertEquals(if (upper) stage.relay.top - 6f * density else stage.relay.bottom + 6f * density,
                        curve.start.y, .001f)
                    assertEquals(far.right + 4f * density, curve.end.x, .001f)
                    assertEquals(far.center.y, curve.end.y, .001f)
                    assertEquals(curve.start.x, curve.control1.x, .001f)
                    assertEquals(curve.start.y + .55f * (curve.end.y - curve.start.y), curve.control1.y, .001f)
                    assertEquals(curve.end.x + .55f * (curve.start.x - curve.end.x), curve.control2.x, .001f)
                    assertEquals(curve.end.y, curve.control2.y, .001f)
                    assertEquals(0f, curve.tangent(0f).x, .001f)
                    assertTrue(if (upper) curve.tangent(0f).y < 0f else curve.tangent(0f).y > 0f)
                    assertEquals(0f, curve.tangent(1f).y, .001f)
                    assertTrue(curve.tangent(1f).x < 0f)
                }
            }
        }
    }

    @Test fun numbersFollowTheOutwardNormalAtHalfTheCableLength() {
        listOf(StageLayout.VERTICAL, StageLayout.VERTICAL_COMPACT).forEach { layout ->
            listOf(1f, 2.75f).forEach { density ->
                val stage = stageGeometry(Size(312f * density, layout.height.value * density), layout, density)
                listOf(stage.iphoneCable to stage.iphone, stage.carCable to stage.car).forEach { (cable, far) ->
                    val points = (0..256).map { cable.curve.point(it / 256f) }
                    val lengths = points.zipWithNext { a, b -> (b - a).getDistance() }
                    var remaining = lengths.sum() / 2f
                    var index = 0
                    while (remaining > lengths[index] && index < lengths.lastIndex) remaining -= lengths[index++]
                    val midpoint = points[index] + (points[index + 1] - points[index]) * (remaining / lengths[index])
                    val normal = cable.number.normal
                    val base = cable.number.point - normal * (14f * density)
                    assertEquals(midpoint.x, base.x, .05f * density)
                    assertEquals(midpoint.y, base.y, .05f * density)
                    assertEquals(1f, normal.getDistance(), .001f)
                    val direction = points[index + 1] - points[index]
                    assertTrue(abs(normal.x * direction.x + normal.y * direction.y) / direction.getDistance() < .01f)
                    val away = midpoint - (stage.relay.center + far.center) / 2f
                    assertTrue(normal.x * away.x + normal.y * away.y > 0f)
                    val center = cable.number.center(16f * density)
                    assertEquals(22f * density, (center - base).getDistance(), .001f)
                    assertTrue(if (far == stage.iphone) center.y < midpoint.y else center.y > midpoint.y)
                }
            }
        }
    }
}
