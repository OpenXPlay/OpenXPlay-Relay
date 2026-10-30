// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-30
package org.openxplay.direlay.ui.design

import android.app.Activity
import android.content.Context
import android.content.ContextWrapper
import android.os.PowerManager
import android.view.WindowManager
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.*
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.platform.LocalLifecycleOwner
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.*
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.core.view.WindowCompat
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.LifecycleEventObserver
import org.openxplay.direlay.R

@Composable internal fun StageMinimal(onWake: () -> Unit, manageWindow: Boolean) {
    val line = stringResource(R.string.stage_live_minimal)
    val wake = stringResource(R.string.stage_live_minimal_wake)
    if (manageWindow) StageMinimalWindow()
    Box(Modifier.fillMaxSize().background(Color.Black).clearAndSetSemantics {
        text = AnnotatedString(line)
        role = Role.Button
        onClick(label = wake) { onWake(); true }
    }.safeDrawingPadding().padding(24.dp), contentAlignment = Alignment.Center) {
        Text(line, color = Color.White.copy(alpha = .38f), style = MaterialTheme.typography.titleMedium,
            textAlign = TextAlign.Center)
    }
}

@Suppress("DEPRECATION")
@Composable private fun StageMinimalWindow() {
    val view = LocalView.current
    val lifecycle = LocalLifecycleOwner.current.lifecycle
    val window = remember(view) { view.context.activity()?.window }
    DisposableEffect(window, view, lifecycle) {
        if (window == null) onDispose { } else {
            val bars = WindowCompat.getInsetsController(window, view)
            val lightStatus = bars.isAppearanceLightStatusBars
            val lightNavigation = bars.isAppearanceLightNavigationBars
            val status = window.statusBarColor
            val navigation = window.navigationBarColor
            val power = view.context.getSystemService(PowerManager::class.java)
            val sustained = power?.isSustainedPerformanceModeSupported == true
            var applied = false
            fun restore() {
                if (!applied) return
                applied = false
                window.attributes = window.attributes.apply { screenBrightness = WindowManager.LayoutParams.BRIGHTNESS_OVERRIDE_NONE }
                if (sustained) window.setSustainedPerformanceMode(false)
                window.statusBarColor = status
                window.navigationBarColor = navigation
                bars.isAppearanceLightStatusBars = lightStatus
                bars.isAppearanceLightNavigationBars = lightNavigation
            }
            fun update() {
                if (!lifecycle.currentState.isAtLeast(Lifecycle.State.STARTED)) { restore(); return }
                if (applied) return
                applied = true
                window.attributes = window.attributes.apply { screenBrightness = .01f }
                if (sustained) window.setSustainedPerformanceMode(true)
                window.statusBarColor = android.graphics.Color.BLACK
                window.navigationBarColor = android.graphics.Color.BLACK
                bars.isAppearanceLightStatusBars = false
                bars.isAppearanceLightNavigationBars = false
            }
            // Stop can pause the recomposer before it disposes the minimal surface.
            val observer = LifecycleEventObserver { _, _ -> update() }
            lifecycle.addObserver(observer)
            update()
            onDispose { lifecycle.removeObserver(observer); restore() }
        }
    }
}

private tailrec fun Context.activity(): Activity? = when (this) {
    is Activity -> this
    is ContextWrapper -> baseContext.activity()
    else -> null
}
