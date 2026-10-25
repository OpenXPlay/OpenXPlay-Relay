// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-25
package org.openxplay.direlay.ui

import androidx.compose.animation.core.Animatable
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.*
import org.openxplay.direlay.ui.design.relayEffects
import org.openxplay.direlay.ui.design.relaySpatial

internal fun setupEntranceDelayMillis(order: Int) = order.coerceIn(0, 3) * 40

private class SetupEntranceValues {
    val rise = List(4) { Animatable(1f) }
    val opacity = List(4) { Animatable(0f) }
}
private val LocalSetupEntrance = staticCompositionLocalOf<SetupEntranceValues?> { null }

/** One clock per page appearance; newly disclosed content inherits the page's finished entrance. */
@Composable internal fun SetupPageEntrance(content: @Composable () -> Unit) {
    val values = remember { SetupEntranceValues() }
    val motion = LocalRelayMotion.current
    var appeared by remember { mutableStateOf(false) }
    LaunchedEffect(motion) {
        val animate = motion && !appeared
        appeared = true
        coroutineScope {
            values.rise.indices.forEach { order -> launch {
                if (animate) {
                    delay(setupEntranceDelayMillis(order).toLong())
                    withTimeoutOrNull(280) {
                        coroutineScope {
                            launch { values.rise[order].animateTo(0f, relaySpatial(true)) }
                            launch { values.opacity[order].animateTo(1f, relayEffects(true)) }
                        }
                    }
                }
                values.rise[order].snapTo(0f)
                values.opacity[order].snapTo(1f)
            } }
        }
    }
    CompositionLocalProvider(LocalSetupEntrance provides values, content = content)
}

@Composable internal fun SetupEntrance(order: Int, content: @Composable () -> Unit) {
    val values = LocalSetupEntrance.current
    val motion = LocalRelayMotion.current
    val distance = with(LocalDensity.current) { 12.dp.toPx() }
    val index = order.coerceIn(0, 3)
    Box(Modifier.fillMaxWidth().graphicsLayer {
        translationY = if (motion && values != null) distance * values.rise[index].value else 0f
        alpha = if (motion && values != null) values.opacity[index].value else 1f
    }) { content() }
}

@Composable internal fun PreparePulse(request: Int, focused: Boolean, content: @Composable () -> Unit) {
    val motion = LocalRelayMotion.current
    val emphasis = remember { Animatable(1f) }
    var previousRequest by remember { mutableIntStateOf(0) }
    LaunchedEffect(request, focused, motion) {
        val pulse = request > 0 && request != previousRequest
        previousRequest = request
        emphasis.snapTo(1f)
        if (pulse && focused && motion) repeat(2) {
            emphasis.animateTo(.45f, androidx.compose.animation.core.tween(140))
            emphasis.animateTo(1f, androidx.compose.animation.core.tween(180))
        }
    }
    Box(Modifier.graphicsLayer { alpha = if (motion) emphasis.value else 1f }) { content() }
}
