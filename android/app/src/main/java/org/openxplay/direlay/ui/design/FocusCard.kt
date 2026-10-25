// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-24
package org.openxplay.direlay.ui.design

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import androidx.compose.ui.platform.LocalHapticFeedback
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import android.content.ClipData
import android.content.ClipboardManager
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import org.openxplay.direlay.R
import org.openxplay.direlay.path.*
import org.openxplay.direlay.ui.Symbol

@Composable fun FocusCard(step: PathStep, onAction: (StepAction) -> Unit, onRecheck: () -> Unit, onDetails: () -> Unit, compact: Boolean = false) {
    if (compact) { CompactFocusCard(step, onAction, onRecheck, onDetails); return }
    val color = LocalStatusColors.current.color(step.state)
    Card(Modifier.fillMaxWidth(), shape = RelayTokens.card,
        colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surface),
        border = BorderStroke(1.dp, color.copy(alpha = .35f))) {
        Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                StatusGlyph(step.state, 40.dp)
                Text(stringResource(step.title), Modifier.weight(1f).semantics { heading() }, style = RelayTokens.title)
                IconButton(onDetails) { Symbol(R.drawable.symbol_info, stringResource(R.string.design_details)) }
            }
            step.detail?.let { Text(stepText(it), style = RelayTokens.body) }
            if (step.state == StepState.WORKING || step.state == StepState.WAITING) ElapsedTime(step.sinceMillis)
            step.evidence?.let { EvidenceRow(stepText(it)) }
            Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(8.dp), verticalAlignment = Alignment.CenterVertically) {
                step.action?.let { action -> Button(onClick = { onAction(action) }, modifier = Modifier.weight(1f).heightIn(min = 48.dp),
                    shape = RelayTokens.pill) {
                    Text(stringResource(step.actionLabel ?: R.string.design_open), textAlign = TextAlign.Center)
                } }
                TextButton(onRecheck, Modifier.heightIn(min = 48.dp)) { Text(stringResource(R.string.design_check_again), textAlign = TextAlign.Center) }
            }
        }
    }
}

@Composable internal fun EvidenceRow(value: String) {
    val copied = stringResource(R.string.design_copied)
    val context = androidx.compose.ui.platform.LocalContext.current
    val clipboard = context.getSystemService(ClipboardManager::class.java)
    Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
        Text(value, Modifier.weight(1f), style = if (evidenceUsesMonospace(value)) RelayTokens.mono else RelayTokens.caption)
        IconButton({ clipboard.setPrimaryClip(ClipData.newPlainText(copied, value)); android.widget.Toast.makeText(context, copied, android.widget.Toast.LENGTH_SHORT).show() }) {
            Symbol(R.drawable.symbol_copy, stringResource(R.string.ui_copy))
        }
    }
}

@OptIn(ExperimentalLayoutApi::class)
@Composable fun LiveCard(live: LiveSummary, path: ConnectionPath, onStop: () -> Unit, compact: Boolean = false) {
    if (compact) { CompactLiveCard(live, path, onStop); return }
    val seconds = rememberElapsedSeconds(live.sinceMillis)
    val haptic = LocalHapticFeedback.current
    Card(Modifier.fillMaxWidth(), shape = RelayTokens.card,
        colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surface),
        border = BorderStroke(1.dp, LocalStatusColors.current.ok.copy(alpha = .35f))) {
        Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                StatusGlyph(StepState.OK, 40.dp)
                Text(stringResource(R.string.design_live), Modifier.weight(1f).semantics { heading() }, style = RelayTokens.title)
                Text(sessionTime(seconds), style = RelayTokens.mono)
            }
            FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                live.chips.forEach { chip -> Surface(shape = RelayTokens.chip, color = LocalStatusColors.current.container(StepState.OK)) {
                    Text(chip, Modifier.padding(horizontal = 8.dp, vertical = 4.dp),
                        style = if (evidenceUsesMonospace(chip)) RelayTokens.mono else RelayTokens.caption)
                } }
            }
            path.nodes.car?.let { Text(it, style = RelayTokens.footnote,
                color = MaterialTheme.colorScheme.onSurfaceVariant) }
            TextButton({ haptic.performHapticFeedback(HapticFeedbackType.LongPress); onStop() }, Modifier.heightIn(min = 48.dp)) { Text(stringResource(R.string.design_stop), textAlign = TextAlign.Center) }
        }
    }
}

/** The short window keeps evidence and actions visible beside the section list. */
@Composable private fun CompactFocusCard(step: PathStep, onAction: (StepAction) -> Unit,
    onRecheck: () -> Unit, onDetails: () -> Unit) {
    val color = LocalStatusColors.current.color(step.state)
    val context = androidx.compose.ui.platform.LocalContext.current
    val copied = stringResource(R.string.design_copied)
    CompositionLocalProvider(LocalMinimumInteractiveComponentSize provides 40.dp) {
        Card(Modifier.fillMaxWidth(), shape = RelayTokens.card,
            colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surface),
            border = BorderStroke(1.dp, color.copy(alpha = .35f))) {
            Column(Modifier.padding(8.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    StatusGlyph(step.state, 32.dp)
                    Column(Modifier.weight(1f)) {
                        Text(stringResource(step.title), style = RelayTokens.headline)
                        step.detail?.let { Text(stepText(it), style = RelayTokens.caption, color = MaterialTheme.colorScheme.onSurfaceVariant) }
                    }
                    IconButton(onDetails, Modifier.size(40.dp)) { Symbol(R.drawable.symbol_info, stringResource(R.string.design_details)) }
                }
                Row(verticalAlignment = Alignment.CenterVertically) {
                    step.evidence?.let {
                        val value = stepText(it)
                        Text(value, Modifier.weight(1f).clickable {
                            context.getSystemService(ClipboardManager::class.java).setPrimaryClip(ClipData.newPlainText(copied, value))
                            android.widget.Toast.makeText(context, copied, android.widget.Toast.LENGTH_SHORT).show()
                        }, style = if (evidenceUsesMonospace(value)) RelayTokens.mono else RelayTokens.caption)
                    }
                    if (step.evidence == null) step.code?.let { Text(it, Modifier.weight(1f), style = RelayTokens.mono) }
                    if (step.state == StepState.WORKING || step.state == StepState.WAITING) ElapsedTime(step.sinceMillis)
                }
                Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(4.dp)) {
                    step.action?.let { action -> Button({ onAction(action) }, Modifier.weight(1f).heightIn(min = 40.dp), shape = RelayTokens.pill,
                        contentPadding = PaddingValues(horizontal = 12.dp, vertical = 4.dp)) {
                        Text(stringResource(step.actionLabel ?: R.string.design_open), style = RelayTokens.footnote, textAlign = TextAlign.Center)
                    } }
                    TextButton(onRecheck, Modifier.heightIn(min = 40.dp), contentPadding = PaddingValues(horizontal = 8.dp, vertical = 4.dp)) {
                        Text(stringResource(R.string.design_check_again), style = RelayTokens.footnote, textAlign = TextAlign.Center)
                    }
                }
            }
        }
    }
}

@Composable private fun CompactLiveCard(live: LiveSummary, path: ConnectionPath, onStop: () -> Unit) {
    val seconds = rememberElapsedSeconds(live.sinceMillis)
    val haptic = LocalHapticFeedback.current
    CompositionLocalProvider(LocalMinimumInteractiveComponentSize provides 40.dp) {
        Card(Modifier.fillMaxWidth(), shape = RelayTokens.card,
            colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surface),
            border = BorderStroke(1.dp, LocalStatusColors.current.ok.copy(alpha = .35f))) {
            Column(Modifier.padding(8.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                Row(Modifier.heightIn(min = 40.dp), verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    StatusGlyph(StepState.OK, 32.dp)
                    Text(stringResource(R.string.design_live), Modifier.weight(1f), style = RelayTokens.headline)
                    Text(sessionTime(seconds), style = RelayTokens.mono)
                }
                val evidence = live.chips.joinToString(" · ")
                Text(evidence, style = if (evidenceUsesMonospace(evidence)) RelayTokens.mono else RelayTokens.caption)
                Row(Modifier.heightIn(min = 40.dp), verticalAlignment = Alignment.CenterVertically) {
                    Text(path.nodes.car.orEmpty(), Modifier.weight(1f), style = RelayTokens.caption, color = MaterialTheme.colorScheme.onSurfaceVariant)
                    TextButton({ haptic.performHapticFeedback(HapticFeedbackType.LongPress); onStop() }) { Text(stringResource(R.string.design_stop), textAlign = TextAlign.Center) }
                }
            }
        }
    }
}
