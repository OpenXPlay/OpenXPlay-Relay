// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-15
package org.openxplay.direlay.ui

import android.database.ContentObserver
import android.os.Handler
import android.os.Looper
import android.provider.Settings
import androidx.compose.animation.*
import org.openxplay.direlay.ui.design.relaySpatial
import org.openxplay.direlay.ui.design.relayEffects
import androidx.compose.animation.core.CubicBezierEasing
import androidx.compose.runtime.*
import androidx.compose.ui.platform.LocalContext
import androidx.compose.material3.Icon
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.painterResource

const val RelayPageDurationMillis = 300
val RelayPageEasing = CubicBezierEasing(.2f, 0f, 0f, 1f)
fun relayPageTransition(forward: Boolean, motion: Boolean): ContentTransform {
    if (!motion) return EnterTransition.None togetherWith ExitTransition.None
    return (slideInHorizontally(relaySpatial(motion)) { if (forward) it else -it * 3 / 10 } +
        fadeIn(relayEffects(motion), initialAlpha = if (forward) 1f else .9f)) togetherWith
        (slideOutHorizontally(relaySpatial(motion)) { if (forward) -it * 3 / 10 else it } +
            fadeOut(relayEffects(motion), targetAlpha = if (forward) .9f else 1f))
}

val LocalRelayMotion = staticCompositionLocalOf { true }
@Composable fun rememberRelayMotion(): Boolean {
    val resolver = LocalContext.current.contentResolver
    fun enabled() = Settings.Global.getFloat(resolver, Settings.Global.ANIMATOR_DURATION_SCALE, 1f) != 0f
    var motion by remember { mutableStateOf(enabled()) }
    DisposableEffect(resolver) {
        val observer = object : ContentObserver(Handler(Looper.getMainLooper())) {
            override fun onChange(selfChange: Boolean) { motion = enabled() }
        }
        resolver.registerContentObserver(Settings.Global.getUriFor(Settings.Global.ANIMATOR_DURATION_SCALE), false, observer)
        onDispose { resolver.unregisterContentObserver(observer) }
    }
    return motion
}
@Composable fun Symbol(resource: Int, description: String?, modifier: Modifier = Modifier) {
    Icon(painterResource(resource), description, modifier)
}
