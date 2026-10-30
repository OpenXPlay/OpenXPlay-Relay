// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-25
package org.openxplay.direlay.ui.design

import android.os.Build
import android.view.HapticFeedbackConstants
import androidx.compose.animation.*
import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.LinearEasing
import androidx.compose.animation.core.tween
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.drawWithCache
import androidx.compose.ui.draw.drawWithContent
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.graphics.*
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.*
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.launch
import org.openxplay.direlay.R
import org.openxplay.direlay.path.*
import org.openxplay.direlay.ui.*

@Composable fun ConnectionStage(path: ConnectionPath, nodes: StageNodes,
    modifier: Modifier = Modifier, onNode: (PathSection) -> Unit) {
    val motion = LocalRelayMotion.current
    val pulse = remember { Animatable(-1f) }
    var pulsed by androidx.compose.runtime.saveable.rememberSaveable { mutableStateOf(false) }
    LaunchedEffect(nodes.live, motion) {
        pulse.snapTo(-1f)
        if (!nodes.live) pulsed = false
        if (nodes.live && !pulsed) {
            pulsed = true
            if (motion) repeat(2) {
                pulse.snapTo(0f)
                pulse.animateTo(1f, tween(1200, easing = LinearEasing))
            }
        }
        pulse.snapTo(-1f)
    }
    Row(modifier.fillMaxWidth().heightIn(min = 112.dp), verticalAlignment = Alignment.CenterVertically) {
        StageTile(StageNode.IPHONE, nodes.iphone, path, nodes.focus, onNode)
        StageCable(nodes.iphoneLink, true, nodes.live, pulse.asState(), Modifier.weight(1f))
        StageTile(StageNode.RELAY, nodes.relay, path, nodes.focus, onNode, nodes.relayLink)
        StageCable(nodes.carLink, false, nodes.live, pulse.asState(), Modifier.weight(1f))
        StageTile(StageNode.CAR, nodes.car, path, nodes.focus, onNode)
    }
}

@Composable private fun StageTile(node: StageNode, state: StepState, path: ConnectionPath,
    focus: StageNode?, onNode: (PathSection) -> Unit, relayLink: LinkFill? = null) {
    val motion = LocalRelayMotion.current
    val view = LocalView.current
    val centerNode = node == StageNode.RELAY
    val arrival = stageArrival(if (centerNode) 0 else if (node == StageNode.IPHONE) 60 else 120)
    val progress = stageFloat(if (relayLink != null && relayLink.total > 0) relayLink.done.toFloat() / relayLink.total else 0f)
    val progressColor = LocalStatusColors.current.color(relayLink?.state ?: StepState.BLOCKED)
    val strength = stageFloat(if (focus == null || focus == node) 1f else .55f)
    val scale = stageFloat(if (focus == null || focus == node) 1f else .92f)
    val ring = remember { Animatable(1f) }
    val shake = remember { Animatable(0f) }
    val sections = path.sections.filter { when (node) {
        StageNode.IPHONE -> it.section == PathSection.IPHONE
        StageNode.RELAY -> it.section == PathSection.RELAY
        StageNode.CAR -> it.section in setOf(PathSection.CAR, PathSection.PICTURE)
    } }
    val states = sections.associate { it.section to it.state }
    val failed = sections.flatMap { it.steps }.filter { it.state == StepState.FAILED }.map { it.id }.toSet()
    var previousStates by remember { mutableStateOf(states) }
    var previousFailed by remember { mutableStateOf(failed) }
    LaunchedEffect(states, failed, motion) {
        val complete = states.any { (section, value) -> value == StepState.OK && previousStates[section]?.let { it != StepState.OK } == true }
        val failure = (failed - previousFailed).isNotEmpty()
        previousStates = states
        previousFailed = failed
        ring.snapTo(1f)
        shake.snapTo(0f)
        if (Build.VERSION.SDK_INT >= 30 && (complete || failure)) view.performHapticFeedback(
            if (failure) HapticFeedbackConstants.REJECT else HapticFeedbackConstants.CONFIRM)
        if (motion) coroutineScope {
            if (complete) launch { ring.snapTo(0f); ring.animateTo(1f, tween(400)) }
            if (failure) launch {
                repeat(2) {
                    shake.animateTo(-4f, tween(60))
                    shake.animateTo(4f, tween(60))
                }
                shake.animateTo(0f, tween(60))
            }
        }
    }
    val role = stringResource(when (node) {
        StageNode.IPHONE -> R.string.network_iphone
        StageNode.RELAY -> R.string.network_this_phone
        StageNode.CAR -> R.string.network_car
    })
    val name = when (node) {
        StageNode.IPHONE -> path.nodes.iphone
        StageNode.RELAY -> path.nodes.relay
        StageNode.CAR -> path.nodes.car
    }
    val label = stringResource(stateLabel(state))
    val description = listOfNotNull(role, label, name).joinToString(", ")
    val section = when (node) {
        StageNode.IPHONE -> PathSection.IPHONE
        StageNode.RELAY -> PathSection.RELAY
        StageNode.CAR -> if (focusForPage(path)?.id?.section == PathSection.PICTURE ||
            path.section(PathSection.CAR) == null) PathSection.PICTURE else PathSection.CAR
    }
    val tint = if (centerNode) iconHue(IconHue.NETWORK) else MaterialTheme.colorScheme.onSurface
    val tile = MaterialTheme.colorScheme.surfaceContainerHigh
    val ok = LocalStatusColors.current.ok
    val transform = if (motion) ContentTransform(
        fadeIn(tween(180)) + scaleIn(tween(220), initialScale = .6f),
        fadeOut(tween(160)) + scaleOut(tween(180), targetScale = .6f), sizeTransform = null)
        else ContentTransform(EnterTransition.None, ExitTransition.None, sizeTransform = null)
    val ringTrack = MaterialTheme.colorScheme.outlineVariant
    val progressRing = if (centerNode) Modifier.drawWithCache {
        val outset = 4.dp.toPx()
        // Concentric with the tile's corners, so the ring reads as progress rather than a bracket.
        val radius = RelayTokens.group.topStart.toPx(size, this) + outset
        val left = -outset
        val top = -outset
        val right = size.width + outset
        val bottom = size.height + outset
        val outline = Path().apply {
            moveTo(size.width / 2f, top)
            lineTo(right - radius, top)
            arcTo(Rect(right - 2 * radius, top, right, top + 2 * radius), -90f, 90f, false)
            lineTo(right, bottom - radius)
            arcTo(Rect(right - 2 * radius, bottom - 2 * radius, right, bottom), 0f, 90f, false)
            lineTo(left + radius, bottom)
            arcTo(Rect(left, bottom - 2 * radius, left + 2 * radius, bottom), 90f, 90f, false)
            lineTo(left, top + radius)
            arcTo(Rect(left, top, left + 2 * radius, top + 2 * radius), 180f, 90f, false)
            close()
        }
        val measure = PathMeasure().apply { setPath(outline, true) }
        val length = measure.length
        val segment = Path()
        val stroke = Stroke(2.dp.toPx(), cap = StrokeCap.Round)
        onDrawWithContent {
            drawPath(outline, ringTrack.copy(alpha = .6f * arrival.value), style = stroke)
            segment.reset()
            measure.getSegment(0f, length * (progress.value * arrival.value).coerceIn(0f, 1f), segment)
            drawPath(segment, progressColor.copy(alpha = if (relayLink?.state == StepState.OK) .5f else 1f), style = stroke)
            drawContent()
        }
    } else Modifier
    Box(Modifier.size(if (centerNode) 64.dp else 56.dp).graphicsLayer {
        // Per-draw alpha: an offscreen layer would clip the badge and the completion ring to the tile.
        compositingStrategy = CompositingStrategy.ModulateAlpha
        alpha = strength.value * arrival.value
        val arrivingScale = if (centerNode) .8f + .2f * arrival.value else 1f
        scaleX = scale.value * arrivingScale
        scaleY = scale.value * arrivingScale
        translationX = shake.value * density + if (centerNode) 0f else
            (if (node == StageNode.IPHONE) 24f else -24f) * density * (1f - arrival.value)
    }.then(progressRing).drawWithContent {
        if (ring.value < 1f) drawRoundRect(ok.copy(alpha = .4f * (1f - ring.value)),
            topLeft = Offset(-20.dp.toPx() * ring.value, -20.dp.toPx() * ring.value),
            size = androidx.compose.ui.geometry.Size(size.width + 40.dp.toPx() * ring.value, size.height + 40.dp.toPx() * ring.value),
            cornerRadius = androidx.compose.ui.geometry.CornerRadius(20.dp.toPx() + 20.dp.toPx() * ring.value),
            style = Stroke(2.dp.toPx()))
        drawContent()
    }.background(tile, RelayTokens.group).semantics(mergeDescendants = true) { contentDescription = description }
        .clickable(role = Role.Button) { onNode(section) }, contentAlignment = Alignment.Center) {
        CompositionLocalProvider(androidx.compose.material3.LocalContentColor provides tint) {
            Symbol(if (node == StageNode.CAR) R.drawable.symbol_car_front else R.drawable.symbol_smartphone,
                null, Modifier.size(if (centerNode) 32.dp else 28.dp))
        }
        AnimatedContent(state, Modifier.align(Alignment.TopEnd).offset(x = 6.dp, y = (-6).dp),
            transitionSpec = { transform }, label = "stage badge") { badge ->
            if (badge != StepState.OK) {
                // The stage owns all bounded motion and feedback, including on older glyph implementations.
                CompositionLocalProvider(LocalRelayMotion provides false) { StatusGlyph(badge, 20.dp, announce = false) }
            } else Spacer(Modifier.size(20.dp))
        }
    }
}
