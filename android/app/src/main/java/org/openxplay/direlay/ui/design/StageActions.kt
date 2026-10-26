// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-25
package org.openxplay.direlay.ui.design

import android.os.SystemClock
import androidx.compose.animation.*
import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.LinearEasing
import androidx.compose.animation.core.tween
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.collectIsPressedAsState
import androidx.compose.foundation.layout.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.geometry.RoundRect
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.*
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.*
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import org.openxplay.direlay.R
import org.openxplay.direlay.path.StepAction
import org.openxplay.direlay.ui.*

const val STAGE_COUNTDOWN_SECONDS = 5
internal val LocalStageCountdownDeadline = staticCompositionLocalOf<Long?> { null }

/** Stop and Connect swap in place, so a double tap on one must not reach the other. */
internal const val STAGE_TAP_SETTLE_MILLIS = 500L
internal fun stageTapSettled(changedAtMillis: Long, nowMillis: Long): Boolean =
    nowMillis - changedAtMillis >= STAGE_TAP_SETTLE_MILLIS

@Composable private fun rememberSettledAction(action: StageAction?, onAction: (StageAction) -> Unit): (StageAction) -> Unit {
    val changedAt = remember(action) { SystemClock.uptimeMillis() }
    val latest by rememberUpdatedState(onAction)
    return remember(changedAt) { { tapped -> if (stageTapSettled(changedAt, SystemClock.uptimeMillis())) latest(tapped) } }
}

@Composable fun StageActions(view: StageView, onAction: (StageAction) -> Unit,
    onCountdownComplete: () -> Unit, modifier: Modifier = Modifier, sideBySide: Boolean = false) {
    val motion = LocalRelayMotion.current
    val primaryAction = rememberSettledAction(view.primary, onAction)
    val secondaryAction = rememberSettledAction(view.secondary, onAction)
    val primary: @Composable (Modifier) -> Unit = { slot ->
        AnimatedVisibility(view.primary != null, slot,
            enter = if (motion) fadeIn(tween(200)) + scaleIn(tween(250), initialScale = .9f) else EnterTransition.None,
            exit = if (motion) fadeOut(tween(180)) + scaleOut(tween(200), targetScale = .9f) else ExitTransition.None) {
            // Keep the outgoing label while its pill disappears.
            var last by remember { mutableStateOf(view.primary ?: StageAction.Connect) }
            SideEffect { view.primary?.let { last = it } }
            StagePrimaryPill(view.primary ?: last, view.primaryEnabled && view.primary != null,
                countdown = view.countdown && view.primary == StageAction.Connect,
                onAction = primaryAction, onCountdownComplete = onCountdownComplete)
        }
    }
    val transform = stageTextTransform(motion)
    val secondary: @Composable () -> Unit = {
        AnimatedContent(view.secondary, transitionSpec = { transform }, label = "stage secondary") { action ->
            if (action != null) TextButton({ secondaryAction(action) }, Modifier.heightIn(min = 48.dp)) {
                Text(stageActionLabel(action), style = RelayTokens.callout, textAlign = TextAlign.Center)
            }
        }
    }
    // A short window has no height for a second row, so the quiet action sits beside the pill.
    if (sideBySide) Row(modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        if (view.primary != null) primary(Modifier.weight(1f)) else primary(Modifier)
        secondary()
    } else Column(modifier.fillMaxWidth(), horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(4.dp)) {
        primary(Modifier)
        secondary()
    }
}

@Composable fun stageActionLabel(action: StageAction): String = stringResource(when (action) {
    StageAction.Connect -> R.string.home2_connect
    StageAction.Stop -> R.string.design_stop
    StageAction.NotNow -> R.string.stage_not_now
    is StageAction.Step -> action.label ?: R.string.design_open
})

@Composable fun StagePrimaryPill(action: StageAction, enabled: Boolean, countdown: Boolean = false,
    modifier: Modifier = Modifier, onAction: (StageAction) -> Unit, onCountdownComplete: () -> Unit = {}) {
    val motion = LocalRelayMotion.current
    val colors = LocalStatusColors.current
    val retry = action is StageAction.Step && action.action == StepAction.RETRY
    val color = animateColorAsState(if (retry) colors.action.copy(alpha = .16f) else colors.ok,
        relayEffects(motion), label = "stage pill color")
    val width = stageFloat(if (action == StageAction.Connect) 1f else .94f, 350)
    val interactions = remember { MutableInteractionSource() }
    val pressed by interactions.collectIsPressedAsState()
    val press = stageFloat(if (pressed) .97f else 1f, 350)
    val arrival = stageArrival()
    val transform = stageTextTransform(motion)
    val ink = if (retry) colors.action else MaterialTheme.colorScheme.onPrimary
    Box(modifier.fillMaxWidth().heightIn(min = 56.dp).graphicsLayer {
        alpha = if (enabled) arrival.value else arrival.value * .38f
        scaleX = press.value
        scaleY = press.value
        translationY = 16.dp.toPx() * (1f - arrival.value)
    }.drawBehind {
        val pillWidth = size.width * width.value
        drawRoundRect(color.value, topLeft = Offset((size.width - pillWidth) / 2, 0f),
            size = Size(pillWidth, size.height), cornerRadius = CornerRadius(size.height / 2))
    }.semantics(mergeDescendants = true) { if (!enabled) disabled() }
        .clickable(interactionSource = interactions, indication = null, enabled = enabled, role = Role.Button) { onAction(action) },
        contentAlignment = Alignment.Center) {
        CompositionLocalProvider(LocalContentColor provides ink) {
            AnimatedContent(action, transitionSpec = { transform }, label = "stage pill label") { shown ->
                Row(Modifier.padding(horizontal = 24.dp, vertical = 16.dp), verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    if (shown == StageAction.Connect || shown == StageAction.Stop) Symbol(
                        if (shown == StageAction.Connect) R.drawable.symbol_power else R.drawable.symbol_square,
                        null, Modifier.size(20.dp))
                    Text(stageActionLabel(shown), style = RelayTokens.headline, textAlign = TextAlign.Center)
                }
            }
            if (countdown && enabled) StageCountdownRing(onCountdownComplete, Modifier.matchParentSize())
        }
    }
}

@Composable private fun StageCountdownRing(onComplete: () -> Unit, modifier: Modifier) {
    val motion = LocalRelayMotion.current
    val complete by rememberUpdatedState(onComplete)
    val suppliedDeadline = LocalStageCountdownDeadline.current
    val deadline = rememberSaveable(suppliedDeadline) { suppliedDeadline ?: (System.currentTimeMillis() + STAGE_COUNTDOWN_SECONDS * 1000L) }
    var completed by rememberSaveable { mutableStateOf(false) }
    var seconds by remember { mutableIntStateOf(STAGE_COUNTDOWN_SECONDS) }
    val sweep = remember { Animatable(0f) }
    LaunchedEffect(deadline, motion) {
        val remaining = (deadline - System.currentTimeMillis()).coerceIn(0, STAGE_COUNTDOWN_SECONDS * 1000L)
        sweep.snapTo(1f - remaining / (STAGE_COUNTDOWN_SECONDS * 1000f))
        coroutineScope {
            if (motion && remaining > 0) launch { sweep.animateTo(1f, tween(remaining.toInt(), easing = LinearEasing)) }
            // Finite, and only this small semantics node ticks once a second.
            repeat(STAGE_COUNTDOWN_SECONDS + 1) {
                val millis = (deadline - System.currentTimeMillis()).coerceAtLeast(0)
                seconds = ((millis + 999) / 1000).toInt().coerceAtMost(STAGE_COUNTDOWN_SECONDS)
                if (!motion) sweep.snapTo(1f - millis.coerceAtMost(5000) / 5000f)
                if (millis == 0L) {
                    if (!completed) { completed = true; complete() }
                    return@coroutineScope
                }
                delay(minOf(1000L, millis))
            }
        }
    }
    val description = stringResource(R.string.stage_auto_in, seconds)
    val ink = LocalContentColor.current.copy(alpha = .65f)
    val outline = remember { Path() }
    val segment = remember { Path() }
    val measure = remember { PathMeasure() }
    Canvas(modifier.semantics { contentDescription = description }) {
        val inset = 4.dp.toPx()
        outline.reset()
        outline.addRoundRect(RoundRect(Rect(inset, inset, size.width - inset, size.height - inset), CornerRadius(size.height / 2)))
        measure.setPath(outline, true)
        segment.reset()
        measure.getSegment(0f, measure.length * (1f - sweep.value).coerceIn(0f, 1f), segment)
        drawPath(segment, ink, style = Stroke(2.dp.toPx(), cap = StrokeCap.Round))
    }
}

@Composable fun NoticeChip(notice: StageNotice?, onNotice: (StageNotice) -> Unit, modifier: Modifier = Modifier) {
    val motion = LocalRelayMotion.current
    val transform = if (motion) ContentTransform(expandVertically(tween(250)) + fadeIn(tween(200)),
        shrinkVertically(tween(250)) + fadeOut(tween(200)), sizeTransform = SizeTransform { _, _ -> tween(250) })
        else ContentTransform(EnterTransition.None, ExitTransition.None, sizeTransform = null)
    AnimatedContent(notice, modifier, transitionSpec = { transform }, label = "stage notice") { shown ->
        if (shown != null) {
            val label = stringResource(when (shown) {
                StageNotice.DISPLAY_CHANGED -> R.string.stage_notice_display
                StageNotice.CHOOSE_CAR -> R.string.stage_notice_car
                StageNotice.SET_UP -> R.string.home2_setup
            })
            TextButton({ onNotice(shown) }, Modifier.heightIn(min = 48.dp), shape = RelayTokens.pill) {
                Symbol(if (shown == StageNotice.SET_UP) R.drawable.symbol_settings_2 else R.drawable.symbol_car_front,
                    null, Modifier.size(18.dp))
                Spacer(Modifier.width(8.dp))
                Text(label, Modifier.weight(1f, fill = false), style = RelayTokens.footnote, textAlign = TextAlign.Center)
                Spacer(Modifier.width(8.dp))
                Symbol(R.drawable.symbol_chevron, null, Modifier.size(16.dp))
            }
        }
    }
}
