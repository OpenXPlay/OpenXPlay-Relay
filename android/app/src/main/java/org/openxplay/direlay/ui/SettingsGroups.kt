// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-24
package org.openxplay.direlay.ui

import androidx.compose.animation.*
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.relocation.BringIntoViewRequester
import androidx.compose.foundation.relocation.bringIntoViewRequester
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.rotate
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.*
import androidx.compose.ui.unit.dp
import org.openxplay.direlay.R
import org.openxplay.direlay.ui.design.*

/** Keeps section disclosure and field deep links while using the inset list components. */
@OptIn(ExperimentalFoundationApi::class)
@Composable internal fun SettingsGroup(key: String, icon: Int, tile: IconHue, title: String, summary: String,
    initiallyExpanded: Boolean = false, targeted: Boolean = false, selected: Boolean? = null,
    error: Boolean = false, footer: String? = null, content: @Composable ColumnScope.() -> Unit) {
    var expanded by rememberSaveable(key) { mutableStateOf(initiallyExpanded || targeted) }
    var located by rememberSaveable(key) { mutableStateOf(false) }
    val requester = remember { BringIntoViewRequester() }
    val motion = LocalRelayMotion.current
    val angle by animateFloatAsState(if (expanded) 90f else 0f, relaySpatial(motion), label = "settings disclosure")
    val disclosure = stringResource(if (expanded) R.string.menu_expanded else R.string.menu_collapsed)
    val errorLabel = stringResource(R.string.settings_advanced_error)
    LaunchedEffect(targeted) {
        if (targeted && !located) {
            expanded = true
            withFrameNanos { }
            requester.bringIntoView()
            located = true
        }
    }
    InsetGroup(footer = footer) {
        Box(Modifier.bringIntoViewRequester(requester).semantics {
            stateDescription = disclosure
            if (selected != null) this.selected = selected
        }) {
            InsetRow(icon, tile, title,
                if (selected == true) stringResource(R.string.menu_selected_summary, summary) else summary,
                onClick = { expanded = !expanded }, trailing = {
                    if (error) CompositionLocalProvider(LocalContentColor provides MaterialTheme.colorScheme.error) {
                        Symbol(R.drawable.symbol_error, errorLabel)
                    }
                    Box(Modifier.rotate(angle)) { Symbol(R.drawable.symbol_chevron, null, Modifier.size(16.dp)) }
                })
        }
        AnimatedVisibility(expanded, enter = expandVertically(relaySpatial(motion)) + fadeIn(relayEffects(motion)),
            exit = shrinkVertically(relaySpatial(motion)) + fadeOut(relayEffects(motion))) {
            Column(Modifier.padding(start = 16.dp, end = 16.dp, bottom = 16.dp),
                verticalArrangement = Arrangement.spacedBy(8.dp), content = content)
        }
    }
}

@Composable internal fun SettingsToggleGroup(title: String, checked: Boolean, icon: Int, tile: IconHue,
    footer: String? = null, enabled: Boolean = true, header: String? = null, onChange: (Boolean) -> Unit) {
    InsetGroup(header = header, footer = footer) { InsetToggleRow(icon, tile, title, checked, onChange, enabled) }
}

@Composable internal fun SettingsToggleRow(icon: Int, tile: IconHue, title: String, checked: Boolean,
    enabled: Boolean = true, onChange: (Boolean) -> Unit) {
    InsetToggleRow(icon, tile, title, checked, onChange, enabled)
}
