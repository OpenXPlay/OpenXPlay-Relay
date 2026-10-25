// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-24
package org.openxplay.direlay.ui.design

import android.os.Build
import android.view.HapticFeedbackConstants
import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.spring
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.drawscope.rotate
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.*
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import org.openxplay.direlay.R
import org.openxplay.direlay.path.StepState
import org.openxplay.direlay.ui.LocalRelayMotion
import kotlin.math.*

internal const val GlyphSamples = 96

/** Polar samples keep the contour's topology fixed throughout a spring morph. */
internal fun glyphRadii(state: StepState): FloatArray = FloatArray(GlyphSamples) { index ->
    val angle = index * 2.0 * PI / GlyphSamples
    when (state) {
        StepState.OK -> (.88 / (abs(cos(angle)).pow(3.4) + abs(sin(angle)).pow(3.4)).pow(1 / 3.4)).toFloat()
        StepState.ACTION -> (.86 / (abs(cos(angle)).pow(5.0) + abs(sin(angle)).pow(5.0)).pow(.2)).toFloat()
        StepState.WORKING -> (.88 + .075 * cos(8 * angle)).toFloat()
        StepState.WAITING -> .94f
        StepState.FAILED -> (.80 + .14 * cos(5 * (angle + PI / 2))).toFloat()
        StepState.BLOCKED -> .36f
    }
}

internal fun interpolateRadii(from: FloatArray, to: FloatArray, fraction: Float, result: FloatArray) {
    require(from.size == to.size && result.size == from.size)
    for (i in result.indices) result[i] = (from[i] + (to[i] - from[i]) * fraction).coerceAtLeast(.01f)
}

internal fun stateLabel(state: StepState) = when (state) {
    StepState.OK -> R.string.design_state_ok
    StepState.WORKING -> R.string.design_state_working
    StepState.WAITING -> R.string.design_state_waiting
    StepState.ACTION -> R.string.design_state_action
    StepState.FAILED -> R.string.design_state_failed
    StepState.BLOCKED -> R.string.design_state_blocked
}

internal suspend fun runRelayPhase(periodMillis: Long, durationMillis: Long, publish: (Float) -> Unit) {
    val start = withFrameNanos { it }
    var elapsed = 0L
    while (elapsed < durationMillis * 1_000_000L) {
        withFrameNanos { now ->
            elapsed = (now - start).coerceAtLeast(0)
            publish((elapsed.coerceAtMost(durationMillis * 1_000_000L) % (periodMillis * 1_000_000L)) /
                (periodMillis * 1_000_000f))
        }
    }
}

internal suspend fun runRelayBreath(amount: Animatable<Float, androidx.compose.animation.core.AnimationVector1D>) {
    repeat(2) {
        amount.animateTo(.92f, spring(dampingRatio = 1f, stiffness = 20f))
        amount.animateTo(1f, spring(dampingRatio = 1f, stiffness = 20f))
    }
    amount.snapTo(1f)
}

@Composable internal fun rememberRelayPhase(active: Boolean, stateKey: Any? = active,
    periodMillis: Long = 6000, durationMillis: Long = 12000): State<Float> {
    val phase = remember { mutableFloatStateOf(0f) }
    val motion = LocalRelayMotion.current
    LaunchedEffect(active, stateKey, motion, periodMillis, durationMillis) {
        phase.floatValue = 0f
        if (active && motion) runRelayPhase(periodMillis, durationMillis) { phase.floatValue = it }
    }
    return phase
}

@Composable internal fun rememberRelayBreath(active: Boolean, stateKey: Any? = active): State<Float> {
    val amount = remember { Animatable(1f) }
    val motion = LocalRelayMotion.current
    LaunchedEffect(active, stateKey, motion) {
        amount.snapTo(1f)
        if (active && motion) runRelayBreath(amount)
    }
    return amount.asState()
}

@Composable fun StatusGlyph(state: StepState, size: Dp = 28.dp, announce: Boolean = true, stateKey: Any? = state) {
    val motion = LocalRelayMotion.current
    val view = LocalView.current
    var previous by remember { mutableStateOf(state) }
    var from by remember { mutableStateOf(glyphRadii(state)) }
    var target by remember { mutableStateOf(glyphRadii(state)) }
    val progress = remember { Animatable(1f) }
    val radii = remember { FloatArray(GlyphSamples) }
    val directions = remember { FloatArray(GlyphSamples * 2) { i ->
        val angle = (i / 2) * 2.0 * PI / GlyphSamples
        if (i % 2 == 0) cos(angle).toFloat() else sin(angle).toFloat()
    } }
    val outline = remember { Path() }
    val mark = remember { Path() }
    val density = LocalDensity.current
    val stroke = remember(density) { Stroke(width = with(density) { 1.5.dp.toPx() }) }
    val markStroke = remember(size, density) { Stroke(with(density) { size.toPx() } * .065f, cap = StrokeCap.Round) }
    val ringStroke = remember(size, density) { Stroke(with(density) { size.toPx() } * .065f) }
    LaunchedEffect(state, motion) {
        val current = FloatArray(GlyphSamples)
        interpolateRadii(from, target, progress.value, current)
        from = current
        target = glyphRadii(state)
        if (previous != state) {
            if (state == StepState.OK || state == StepState.FAILED) view.performHapticFeedback(
                if (Build.VERSION.SDK_INT >= 30) {
                    if (state == StepState.OK) HapticFeedbackConstants.CONFIRM else HapticFeedbackConstants.REJECT
                } else HapticFeedbackConstants.CLOCK_TICK)
            previous = state
        }
        progress.snapTo(if (motion) 0f else 1f)
        if (motion) progress.animateTo(1f, relaySpatial(true))
    }
    val rotation = rememberRelayPhase(state == StepState.WORKING, stateKey, durationMillis = 8000)
    val breath = rememberRelayBreath(state == StepState.WAITING, stateKey)
    val colors = LocalStatusColors.current
    val color = colors.color(state)
    val label = stringResource(stateLabel(state))
    val description = stringResource(R.string.design_status)
    Canvas(Modifier.size(size).semantics { if (announce) { stateDescription = label; contentDescription = description } }) {
        interpolateRadii(from, target, progress.value, radii)
        val radius = this.size.minDimension * .42f * breath.value
        rotate(if (state == StepState.WORKING) rotation.value * 360f else 0f) {
            outline.reset()
            for (i in radii.indices) {
                val x = center.x + radius * radii[i] * directions[i * 2]
                val y = center.y + radius * radii[i] * directions[i * 2 + 1]
                if (i == 0) outline.moveTo(x, y) else outline.lineTo(x, y)
            }
            outline.close()
            if (state == StepState.BLOCKED) drawPath(outline, color, style = stroke)
            else drawPath(outline, color.copy(alpha = .14f))
        }
        val unit = this.size.minDimension
        val width = unit * .065f
        when (state) {
            StepState.OK -> {
                mark.reset(); mark.moveTo(unit * .31f, unit * .51f)
                mark.lineTo(unit * .45f, unit * .65f); mark.lineTo(unit * .71f, unit * .36f)
                drawPath(mark, color, style = markStroke)
            }
            StepState.WORKING -> drawCircle(color, unit * .07f)
            StepState.WAITING -> drawCircle(color, unit * .12f, style = ringStroke)
            StepState.ACTION -> {
                drawLine(color, Offset(unit * .5f, unit * .3f), Offset(unit * .5f, unit * .55f), width, StrokeCap.Round)
                drawCircle(color, width / 2, Offset(unit * .5f, unit * .7f))
            }
            StepState.FAILED -> {
                drawLine(color, Offset(unit * .37f, unit * .37f), Offset(unit * .63f, unit * .63f), width, StrokeCap.Round)
                drawLine(color, Offset(unit * .63f, unit * .37f), Offset(unit * .37f, unit * .63f), width, StrokeCap.Round)
            }
            StepState.BLOCKED -> Unit
        }
    }
}
