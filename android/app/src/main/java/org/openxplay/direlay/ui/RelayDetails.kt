// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-21
package org.openxplay.direlay.ui

import androidx.compose.animation.animateContentSize
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.tween
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.rotate
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalContext
import org.openxplay.direlay.network.NetworkRequirement
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import org.openxplay.direlay.R

@Composable internal fun HomeProblemRow(problem: HomeProblem) {
    var expanded by remember(problem) { mutableStateOf(false) }
    val ms = if (LocalRelayMotion.current) 220 else 0
    val state = stringResource(if (expanded) R.string.menu_expanded else R.string.menu_collapsed)
    val rotation by animateFloatAsState(if (expanded) 90f else 0f, tween(ms), label = "problem expansion")
    val context = LocalContext.current
    val text = listOfNotNull(problem.textResource?.let { stringResource(it) } ?: problem.text ?: stringResource(R.string.ui_error), problem.detail).joinToString("\n")
    Column(Modifier.fillMaxWidth().heightIn(min = 64.dp)
        .semantics { stateDescription = state }.clickable(role = Role.Button) { expanded = !expanded }
        .animateContentSize(tween(ms)).padding(vertical = 12.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            Text(stringResource(problem.title), Modifier.weight(1f), style = MaterialTheme.typography.titleSmall)
            Symbol(R.drawable.symbol_chevron, state, Modifier.rotate(rotation))
        }
        if (expanded) {
            SelectionContainer { Text(text, style = MaterialTheme.typography.bodyMedium) }
        } else {
            Text(text.replace('\n', ' '), style = MaterialTheme.typography.bodyMedium,
                maxLines = 1, softWrap = false, overflow = TextOverflow.Ellipsis)
        }
        problem.action?.let { action ->
            TextButton(onClick = { NetworkSettingsLauncher(context).open(action == NetworkRequirement.HOTSPOT) },
                modifier = Modifier.heightIn(min = 48.dp)) { Text(stringResource(action.settingsLabel())) }
        }
    }
}

@Composable internal fun FittingAddress(value: String, modifier: Modifier = Modifier) {
    BoxWithConstraints(modifier) {
        val density = LocalDensity.current
        var size by remember(value, maxWidth, density.fontScale) { mutableFloatStateOf(32f) }
        Text(value, fontSize = size.sp, maxLines = 1, softWrap = false, fontFamily = FontFamily.Monospace,
            fontWeight = FontWeight.SemiBold, letterSpacing = 0.sp,
            onTextLayout = {
                if (it.hasVisualOverflow && size > 6f) {
                    val measured = (it.getLineRight(0) - it.getLineLeft(0)).coerceAtLeast(1f)
                    size = (size * (it.size.width / measured) * .98f).coerceIn(6f, (size - .5f).coerceAtLeast(6f))
                }
            })
    }
}

@Composable internal fun ReceiverAddresses(addresses: List<String>, onCopy: (String) -> Unit) {
    addresses.forEach { address ->
        // Keep the scheme visible so typing the address does not try HTTP first.
        Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
            FittingAddress(address.removeSuffix("/"), Modifier.weight(1f))
            IconButton(onClick = { onCopy(address) }, modifier = Modifier.sizeIn(minWidth = 48.dp, minHeight = 48.dp)) {
                Symbol(R.drawable.symbol_copy, stringResource(R.string.ui_copy))
            }
        }
    }
}

@OptIn(ExperimentalLayoutApi::class)
@Composable internal fun DetailValue(label: Int, value: String, monospace: Boolean = false) {
    FlowRow(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(12.dp),
        verticalArrangement = Arrangement.spacedBy(4.dp)) {
        Text(stringResource(label), color = MaterialTheme.colorScheme.onSurfaceVariant,
            style = MaterialTheme.typography.bodyMedium)
        Text(value, style = MaterialTheme.typography.bodyMedium,
            fontFamily = if (monospace) FontFamily.Monospace else FontFamily.Default)
    }
}

@Composable internal fun VideoStatisticsCard(value: VideoStatsPresentation) {
    CollapsibleCard("host-video-statistics", stringResource(R.string.video_stats_title),
        stringResource(value.caption.resource, *value.caption.arguments.toTypedArray())) {
        Hint(stringResource(value.caption.resource, *value.caption.arguments.toTypedArray()))
        value.sections.forEach { section ->
            Text(stringResource(section.title.resource, *section.title.arguments.toTypedArray()),
                style = MaterialTheme.typography.titleSmall)
            section.lines.forEach { Text(stringResource(it.resource, *it.arguments.toTypedArray())) }
        }
    }
}
