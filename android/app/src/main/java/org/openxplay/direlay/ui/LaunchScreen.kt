// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-17
package org.openxplay.direlay.ui

import androidx.activity.compose.BackHandler
import androidx.compose.foundation.Image
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.delay
import org.openxplay.direlay.R

/** Seconds before automatic start continues with the last setup on its own. */
const val LAUNCH_COUNTDOWN = 5

/**
 * Shown each time the app opens with a saved setup: start with the last setup, or set it up again.
 * With automatic start on, the last setup continues by itself after [LAUNCH_COUNTDOWN] seconds.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable fun LaunchScreen(value: HomeContent, onContinue: () -> Unit, onNewSetup: () -> Unit, onLater: () -> Unit) {
    val compact = compactSetup(LocalConfiguration.current.screenHeightDp)
    val buttonHeight = if (compact) 48.dp else 56.dp
    val state = value.presentation
    val countdown = value.automatic && state.canStart && !value.busy
    var remaining by remember { mutableIntStateOf(LAUNCH_COUNTDOWN) }
    if (countdown) LaunchedEffect(Unit) {
        while (remaining > 0) { delay(1_000); remaining-- }
        onContinue()
    }
    // Leaving the choice by Back means "not now", like the button, so automatic start does not run anyway.
    BackHandler(onBack = onLater)
    Scaffold(topBar = { TopAppBar(title = {
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            Image(painterResource(R.mipmap.ic_launcher_foreground), null, Modifier.size(40.dp))
            RelayBrand(Modifier.weight(1f))
        }
    }) }) { padding ->
        Box(Modifier.fillMaxSize().padding(padding).verticalScroll(rememberScrollState()), contentAlignment = Alignment.TopCenter) {
            Column(Modifier.widthIn(max = 640.dp).fillMaxWidth().padding(16.dp), verticalArrangement = Arrangement.spacedBy(16.dp)) {
                Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                    Heading(stringResource(R.string.launch_title))
                    Hint(stringResource(R.string.launch_hint))
                }
                RelayCard {
                    Text(stringResource(R.string.launch_last_title), style = MaterialTheme.typography.titleMedium)
                    Column { ModeSummary(state.mode) }
                    if (!state.canStart) Hint(stringResource(R.string.launch_last_unready))
                    Button(onClick = onContinue, enabled = !value.busy, modifier = Modifier.fillMaxWidth().heightIn(min = buttonHeight),
                        colors = ButtonDefaults.buttonColors(containerColor = RelayGreen)) {
                        if (state.canStart) {
                            Symbol(R.drawable.symbol_play, null, Modifier.size(24.dp))
                            Spacer(Modifier.width(8.dp))
                        }
                        Text(stringResource(if (state.canStart) R.string.launch_continue else R.string.launch_continue_prepare))
                    }
                    if (countdown) Hint(stringResource(R.string.launch_countdown, remaining))
                }
                RelayCard {
                    Text(stringResource(R.string.launch_new_title), style = MaterialTheme.typography.titleMedium)
                    Hint(stringResource(R.string.launch_new_hint))
                    OutlinedButton(onClick = onNewSetup, Modifier.fillMaxWidth().heightIn(min = buttonHeight)) {
                        Text(stringResource(R.string.setup_home_action))
                    }
                }
                TextButton(onClick = onLater, Modifier.align(Alignment.CenterHorizontally).heightIn(min = 48.dp)) {
                    Text(stringResource(R.string.launch_later))
                }
            }
        }
    }
}
