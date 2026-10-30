// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-30
package org.openxplay.direlay.ui

import androidx.compose.runtime.*
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.ui.platform.LocalLifecycleOwner
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.LifecycleEventObserver
import androidx.lifecycle.repeatOnLifecycle
import kotlinx.coroutines.delay
import org.openxplay.direlay.path.LiveSummary

internal const val HIGH_PERFORMANCE_DELAY_MILLIS = 20_000L

/** A null liveSince means the live session ended and its deadline is discarded. */
internal fun highPerformanceActive(enabled: Boolean, liveSince: Long?, lastTouch: Long,
    visible: Boolean, now: Long): Boolean = enabled && visible && liveSince != null &&
    now >= maxOf(liveSince, lastTouch) + HIGH_PERFORMANCE_DELAY_MILLIS

internal data class HighPerformanceMode(val active: Boolean, val visible: Boolean, val touch: () -> Unit)

@Composable internal fun rememberHighPerformance(enabled: Boolean, live: LiveSummary?): HighPerformanceMode {
    val lifecycle = LocalLifecycleOwner.current.lifecycle
    var visible by remember(lifecycle) { mutableStateOf(lifecycle.currentState.isAtLeast(Lifecycle.State.STARTED)) }
    DisposableEffect(lifecycle) {
        val observer = LifecycleEventObserver { _, _ -> visible = lifecycle.currentState.isAtLeast(Lifecycle.State.STARTED) }
        lifecycle.addObserver(observer)
        onDispose { lifecycle.removeObserver(observer) }
    }
    val liveSince = remember(live != null, live?.sinceMillis) {
        live?.let { it.sinceMillis ?: System.currentTimeMillis() }
    }
    var lastTouch by rememberSaveable(liveSince) { mutableLongStateOf(0L) }
    var active by remember(enabled, liveSince, lastTouch, visible) {
        mutableStateOf(highPerformanceActive(enabled, liveSince, lastTouch, visible, System.currentTimeMillis()))
    }
    LaunchedEffect(enabled, liveSince, lastTouch, visible, lifecycle) {
        lifecycle.repeatOnLifecycle(Lifecycle.State.STARTED) {
            if (enabled && liveSince != null && !active) {
                delay((maxOf(liveSince, lastTouch) + HIGH_PERFORMANCE_DELAY_MILLIS - System.currentTimeMillis()).coerceAtLeast(0L))
                active = highPerformanceActive(enabled, liveSince, lastTouch,
                    lifecycle.currentState.isAtLeast(Lifecycle.State.STARTED), System.currentTimeMillis())
            }
        }
    }
    return HighPerformanceMode(active, visible) { lastTouch = System.currentTimeMillis() }
}
