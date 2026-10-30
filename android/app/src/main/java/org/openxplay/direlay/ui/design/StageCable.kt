// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-29
package org.openxplay.direlay.ui.design

import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.LinearOutSlowInEasing
import androidx.compose.animation.core.tween
import androidx.compose.foundation.layout.*
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.drawWithCache
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.*
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.vector.PathParser
import androidx.compose.ui.layout.Layout
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.unit.constrainHeight
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.launch
import org.openxplay.direlay.R
import org.openxplay.direlay.path.StepState
import org.openxplay.direlay.ui.LinkFill
import org.openxplay.direlay.ui.LocalRelayMotion

private val cablePath = PathParser().parsePathString("M0,12 C30,22 70,22 100,12").toPath()

@Composable internal fun StageCable(link: LinkFill, left: Boolean, live: Boolean, pulse: State<Float>, modifier: Modifier) {
    val motion = LocalRelayMotion.current
    val arrival = stageArrival(if (left) 60 else 120)
    val fraction = stageFloat(if (live) 1f else if (link.total > 0) link.done.toFloat() / link.total else 0f)
    val numberAlpha = stageFloat(if (live) 0f else 1f)
    val complete = link.total > 0 && link.done == link.total
    val spark = remember { Animatable(-1f) }
    val click = remember { Animatable(1f) }
    var previousDone by remember { mutableIntStateOf(link.done) }
    var previousComplete by remember { mutableStateOf(complete) }
    LaunchedEffect(link.done, complete, live, motion) {
        val advanced = link.done > previousDone
        val completed = complete && !previousComplete
        previousDone = link.done
        previousComplete = complete
        spark.snapTo(-1f)
        click.snapTo(1f)
        if (motion) coroutineScope {
            if (advanced && !live) launch {
                spark.snapTo(0f)
                spark.animateTo(1f, tween(600, easing = LinearOutSlowInEasing))
                spark.snapTo(-1f)
            }
            if (completed) launch {
                click.animateTo(1.25f, tween(150))
                click.animateTo(1f, tween(150))
            }
        }
    }
    val colors = LocalStatusColors.current
    val color = if (live) colors.ok else colors.color(link.state)
    val track = MaterialTheme.colorScheme.outlineVariant
    val numberColor = when {
        complete -> colors.ok
        link.state == StepState.BLOCKED -> MaterialTheme.colorScheme.onSurfaceVariant
        else -> colors.color(link.state)
    }
    val role = stringResource(if (left) R.string.network_iphone else R.string.network_car)
    val description = stringResource(R.string.stage_link_progress, role, link.done, link.total)
    val currentLink = rememberUpdatedState(link)
    val currentLive = rememberUpdatedState(live)
    val currentMotion = rememberUpdatedState(motion)
    val currentColor = rememberUpdatedState(color)
    val currentColors = rememberUpdatedState(colors)
    val currentTrack = rememberUpdatedState(track)
    // State changes redraw without rebuilding the scaled path or its measure.
    val cable = remember(left) {
        Modifier.fillMaxWidth().height(24.dp).drawWithCache {
            val path = Path().apply {
                addPath(cablePath)
                transform(Matrix().apply { scale(if (left) -size.width / 100f else size.width / 100f, size.height / 24f) })
                if (left) translate(Offset(size.width, 0f))
            }
            val measure = PathMeasure().apply { setPath(path, false) }
            val length = measure.length
            val plugLength = 5.dp.toPx().coerceAtMost(length / 2f)
            val relayPlug = Path().apply { measure.getSegment(0f, plugLength, this) }
            val farPlug = Path().apply { measure.getSegment(length - plugLength, length, this) }
            val segment = Path()
            val stroke = Stroke(3.dp.toPx(), cap = StrokeCap.Round)
            val dashed = Stroke(3.dp.toPx(), cap = StrokeCap.Round,
                pathEffect = PathEffect.dashPathEffect(floatArrayOf(5.dp.toPx(), 4.dp.toPx())))
            val plugStroke = Stroke(6.dp.toPx(), cap = StrokeCap.Butt)
            val gap = 4.dp.toPx()
            val sparkRadius = 10.dp.toPx()
            val pulseRadius = 12.dp.toPx()
            onDrawBehind {
                val link = currentLink.value
                val complete = link.total > 0 && link.done == link.total
                val live = currentLive.value
                val motion = currentMotion.value
                val color = currentColor.value
                val colors = currentColors.value
                val track = currentTrack.value
                drawPath(path, track, style = if (!live && link.state == StepState.BLOCKED) dashed else stroke)
                val fill = (fraction.value * arrival.value).coerceIn(0f, 1f)
                val end = (fill * length - if (!live && link.state == StepState.FAILED) gap else 0f).coerceAtLeast(0f)
                segment.reset()
                if (end > 0f) {
                    measure.getSegment(0f, end, segment)
                    drawPath(segment, color, style = stroke)
                }
                drawPath(relayPlug, if (fill > 0f) color else track, style = plugStroke)
                drawPath(farPlug, if (complete || live) color else track,
                    style = Stroke(6.dp.toPx() * if (motion) click.value else 1f, cap = StrokeCap.Butt))
                if (motion && spark.value >= 0f && end > 0f) {
                    val point = measure.getPosition(spark.value * end)
                    drawCircle(Brush.radialGradient(listOf(color.copy(alpha = .35f), Color.Transparent),
                        center = point, radius = sparkRadius), sparkRadius, point)
                }
                val pass = pulse.value
                val half = if (left) 1f - pass * 2f else (pass - .5f) * 2f
                if (motion && live && pass >= 0f && half in 0f..1f) {
                    val point = measure.getPosition(half * length)
                    drawCircle(Brush.radialGradient(listOf(colors.ok.copy(alpha = .3f), Color.Transparent),
                        center = point, radius = pulseRadius), pulseRadius, point)
                }
            }
        }
    }
    val textTransform = stageTextTransform(motion)
    Layout(modifier = modifier.clearAndSetSemantics { contentDescription = description }, content = {
        AnimatedContent(link.done to link.total, Modifier.graphicsLayer { alpha = numberAlpha.value },
            transitionSpec = { textTransform }, label = "cable progress") { (done, total) ->
            if (total > 0) Text(stringResource(R.string.stage_progress_count, done, total), color = numberColor,
                style = MaterialTheme.typography.labelMedium.copy(fontFeatureSettings = "tnum"))
        }
        Spacer(cable)
    }) { measurables, constraints ->
        val loose = constraints.copy(minWidth = 0, minHeight = 0)
        val number = measurables[0].measure(loose)
        val cable = measurables[1].measure(loose)
        val gap = 4.dp.roundToPx()
        val height = constraints.constrainHeight(maxOf(112.dp.roundToPx(), cable.height + 2 * (number.height + gap)))
        layout(constraints.maxWidth, height) {
            val top = (height - cable.height) / 2
            cable.placeRelative(0, top)
            number.placeRelative((constraints.maxWidth - number.width) / 2, top - gap - number.height)
        }
    }
}
