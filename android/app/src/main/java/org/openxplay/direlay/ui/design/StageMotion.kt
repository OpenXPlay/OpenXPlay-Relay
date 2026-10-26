// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-25
package org.openxplay.direlay.ui.design

import androidx.compose.animation.*
import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.AnimationVector1D
import androidx.compose.animation.core.tween
import androidx.compose.runtime.*
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.delay
import kotlinx.coroutines.withTimeoutOrNull
import org.openxplay.direlay.ui.LocalRelayMotion

internal val LocalStageArrivalEnabled = staticCompositionLocalOf { true }

internal suspend fun Animatable<Float, AnimationVector1D>.stageTo(target: Float, motion: Boolean, bound: Long = 450) {
    if (motion) withTimeoutOrNull(bound) { animateTo(target, relaySpatial(true)) }
    snapTo(target)
}

@Composable internal fun stageFloat(target: Float, bound: Long = 450): State<Float> {
    val motion = LocalRelayMotion.current
    val value = remember { Animatable(target) }
    LaunchedEffect(target, motion) { value.stageTo(target, motion, bound) }
    return value.asState()
}

/** Each arrival slot survives configuration changes and stops within its allotted time. */
@Composable internal fun stageArrival(delayMillis: Long = 0): State<Float> {
    val motion = LocalRelayMotion.current
    val allowed = LocalStageArrivalEnabled.current
    var arrived by rememberSaveable { mutableStateOf(!allowed) }
    val value = remember { Animatable(if (arrived || !motion) 1f else 0f) }
    LaunchedEffect(motion) {
        val play = !arrived && motion && allowed
        arrived = true
        if (play) {
            delay(delayMillis)
            value.stageTo(1f, true, 550)
        } else value.snapTo(1f)
    }
    return value.asState()
}

@Composable internal fun stageTextTransform(motion: Boolean): ContentTransform {
    val rise = with(LocalDensity.current) { 8.dp.roundToPx() }
    return stageTextTransform(motion, rise)
}

internal fun stageTextTransform(motion: Boolean, rise: Int): ContentTransform =
    if (!motion) ContentTransform(EnterTransition.None, ExitTransition.None, sizeTransform = null) else
        ContentTransform(slideInVertically(tween(220)) { rise } + fadeIn(tween(180)),
            slideOutVertically(tween(180)) { -rise } + fadeOut(tween(160)),
            sizeTransform = SizeTransform(clip = false) { _, _ -> tween(220) })
