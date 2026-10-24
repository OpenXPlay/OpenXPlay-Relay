// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-17
package org.openxplay.direlay.ui

import androidx.activity.compose.BackHandler
import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.LinearEasing
import androidx.compose.animation.core.tween
import androidx.compose.foundation.Image
import androidx.compose.foundation.layout.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.delay
import org.openxplay.direlay.R
import org.openxplay.direlay.ui.design.*

/** Seconds before automatic start continues with the last setup on its own. */
const val LAUNCH_COUNTDOWN = 5

/**
 * Shown each time the app opens with a saved setup: start with the last setup, or set it up again.
 * With automatic start on, the last setup continues by itself after [LAUNCH_COUNTDOWN] seconds.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable fun LaunchScreen(value: HomeContent, onContinue: () -> Unit, onNewSetup: () -> Unit, onLater: () -> Unit) {
    val compact = compactSetup(LocalConfiguration.current.screenHeightDp)
    val state = value.presentation
    val countdown = value.automatic && state.canStart && !value.busy
    var remaining by remember { mutableIntStateOf(LAUNCH_COUNTDOWN) }
    val motion = LocalRelayMotion.current
    val progress = remember { Animatable(1f) }
    if (countdown) LaunchedEffect(Unit) {
        while (remaining > 0) { delay(1_000); remaining-- }
        onContinue()
    }
    LaunchedEffect(countdown, motion) {
        progress.snapTo(remaining.toFloat() / LAUNCH_COUNTDOWN)
        if (countdown && motion) progress.animateTo(0f, tween(remaining * 1_000, easing = LinearEasing))
    }
    // Leaving the choice by Back means "not now", like the button, so automatic start does not run anyway.
    BackHandler(onBack = onLater)
    // Landscape has no room for the brand bar: the large title already says where the user is.
    Scaffold(containerColor = MaterialTheme.colorScheme.background, topBar = { if (!compact) TopAppBar(expandedHeight = 64.dp, title = {
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            Image(painterResource(R.mipmap.ic_launcher_foreground), null, Modifier.size(40.dp))
            RelayBrand(Modifier.weight(1f))
        }
    }, colors = TopAppBarDefaults.topAppBarColors(containerColor = MaterialTheme.colorScheme.background)) }, bottomBar = {
        Surface(color = MaterialTheme.colorScheme.surface.copy(alpha = .96f)) {
            Box(Modifier.fillMaxWidth().navigationBarsPadding().padding(horizontal = 16.dp, vertical = if (compact) 8.dp else 12.dp),
                contentAlignment = Alignment.Center) {
                Box(Modifier.widthIn(max = 720.dp).fillMaxWidth()) {
                    PrimaryPill(running = false, enabled = !value.busy, label = stringResource(R.string.setup2_continue), onClick = onContinue)
                    if (countdown) {
                        val description = stringResource(R.string.launch_countdown, remaining)
                        Box(Modifier.align(Alignment.CenterEnd).padding(end = 16.dp).size(32.dp)
                            .clearAndSetSemantics { contentDescription = description; liveRegion = LiveRegionMode.Polite },
                            contentAlignment = Alignment.Center) {
                            CircularProgressIndicator(progress = { if (motion) progress.value else remaining.toFloat() / LAUNCH_COUNTDOWN },
                                modifier = Modifier.fillMaxSize(), color = MaterialTheme.colorScheme.onPrimary,
                                trackColor = MaterialTheme.colorScheme.onPrimary.copy(alpha = .2f), strokeWidth = 2.dp)
                            Text("$remaining", style = RelayTokens.mono, color = MaterialTheme.colorScheme.onPrimary)
                        }
                    }
                }
            }
        }
    }) { padding ->
        Box(Modifier.fillMaxSize().padding(padding).consumeWindowInsets(padding)) {
            val setUpAgain: @Composable (Modifier) -> Unit = { modifier ->
                TextButton(onClick = onNewSetup, modifier.heightIn(min = 48.dp)) { Text(stringResource(R.string.setup2_set_up_again)) }
            }
            val later: @Composable (Modifier) -> Unit = { modifier ->
                TextButton(onClick = onLater, modifier.heightIn(min = 48.dp)) { Text(stringResource(R.string.launch_later)) }
            }
            val choices: @Composable () -> Unit = {
                if (countdown) Hint(stringResource(R.string.launch_countdown, remaining))
                setUpAgain(Modifier.fillMaxWidth())
                later(Modifier.fillMaxWidth())
            }
            // In landscape the choices sit side by side under the title, so nothing needs scrolling.
            SetupPage(compact, stringResource(R.string.setup2_launch_title), stringResource(R.string.setup2_launch_subtitle),
                illustration = { if (compact) Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
                    if (countdown) Hint(stringResource(R.string.launch_countdown, remaining))
                    Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                        setUpAgain(Modifier.weight(1f)); later(Modifier.weight(1f))
                    }
                } }) {
                InsetGroup(header = stringResource(R.string.launch_last_title),
                    footer = if (state.canStart) null else stringResource(R.string.launch_last_unready)) { ModeSummary(state.mode) }
                if (!compact) choices()
            }
        }
    }
}
