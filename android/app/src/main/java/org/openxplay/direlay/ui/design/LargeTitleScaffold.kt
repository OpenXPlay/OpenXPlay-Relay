// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-24
package org.openxplay.direlay.ui.design

import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.*
import androidx.compose.ui.unit.dp
import org.openxplay.direlay.R
import org.openxplay.direlay.ui.Symbol

internal fun largeTitleCollapseFraction(scrollPx: Float, collapsePx: Float, shortWindow: Boolean): Float =
    if (shortWindow || collapsePx <= 0f) 1f else (scrollPx / collapsePx).coerceIn(0f, 1f)

@Composable fun LargeTitleScaffold(title: String, onBack: () -> Unit, actions: @Composable RowScope.() -> Unit = {},
    content: @Composable ColumnScope.() -> Unit) {
    val scroll = rememberScrollState()
    val short = LocalConfiguration.current.screenHeightDp < 480
    val density = LocalDensity.current
    val collapsePx = with(density) { 64.dp.toPx() }
    val fraction by remember(short, collapsePx) { derivedStateOf { largeTitleCollapseFraction(scroll.value.toFloat(), collapsePx, short) } }
    Surface(Modifier.fillMaxSize(), color = MaterialTheme.colorScheme.background) {
        Column(Modifier.fillMaxSize().safeDrawingPadding().imePadding()) {
            Row(Modifier.fillMaxWidth().heightIn(min = 52.dp).padding(horizontal = 4.dp), verticalAlignment = Alignment.CenterVertically) {
                IconButton(onBack) { Symbol(R.drawable.symbol_back, stringResource(R.string.ui_back)) }
                Text(title, Modifier.weight(1f).graphicsLayer { alpha = fraction }.semantics { if (fraction < .5f) hideFromAccessibility() else heading() },
                    style = RelayTokens.headline)
                actions()
            }
            Box(Modifier.fillMaxSize(), contentAlignment = Alignment.TopCenter) {
                Column(Modifier.widthIn(max = 720.dp).fillMaxWidth().verticalScroll(scroll).padding(horizontal = 16.dp),
                    verticalArrangement = Arrangement.spacedBy(12.dp)) {
                    if (!short) Box(Modifier.fillMaxWidth().heightIn(min = 64.dp), contentAlignment = Alignment.CenterStart) {
                        Text(title, Modifier.graphicsLayer { alpha = 1f - fraction }.semantics { heading(); if (fraction >= .5f) hideFromAccessibility() },
                            style = RelayTokens.largeTitle)
                    }
                    content()
                    Spacer(Modifier.height(16.dp))
                }
            }
        }
    }
}
