// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-24
package org.openxplay.direlay.ui.design

import android.content.ClipData
import android.content.ClipboardManager
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.ui.window.Dialog
import androidx.compose.ui.window.DialogProperties
import androidx.compose.ui.window.DialogWindowProvider
import androidx.compose.ui.platform.LocalView
import org.openxplay.direlay.ui.LocalRelayMotion
import org.openxplay.direlay.ui.Symbol
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import org.openxplay.direlay.R
import org.openxplay.direlay.path.*
import java.text.DateFormat
import java.util.Date

@Composable fun RelaySheet(onDismiss: () -> Unit, content: @Composable ColumnScope.() -> Unit) {
    RelaySurface { RelaySheetContent(onDismiss, content) }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable private fun RelaySheetContent(onDismiss: () -> Unit, content: @Composable ColumnScope.() -> Unit) {
    val body: @Composable ColumnScope.() -> Unit = {
        Box(Modifier.fillMaxWidth(), contentAlignment = Alignment.TopCenter) {
            Column(Modifier.widthIn(max = 720.dp).fillMaxWidth().verticalScroll(rememberScrollState())
                .padding(start = 20.dp, end = 20.dp, bottom = 24.dp), verticalArrangement = Arrangement.spacedBy(12.dp), content = content)
        }
    }
    if (!LocalRelayMotion.current) {
        // ModalBottomSheet's entrance animation has no public animation spec in Material 3 1.4.0.
        Dialog(onDismissRequest = onDismiss, properties = DialogProperties(usePlatformDefaultWidth = false, decorFitsSystemWindows = false)) {
            val window = (LocalView.current.parent as? DialogWindowProvider)?.window
            SideEffect { window?.setWindowAnimations(0) }
            Box(Modifier.fillMaxSize().background(MaterialTheme.colorScheme.scrim.copy(alpha = .32f)),
                contentAlignment = Alignment.BottomCenter) {
                Box(Modifier.matchParentSize().clickable(onClick = onDismiss))
                Surface(Modifier.fillMaxWidth().statusBarsPadding().navigationBarsPadding(),
                    shape = RelayTokens.sheet, color = MaterialTheme.colorScheme.surfaceContainerHigh) {
                    Column { BottomSheetDefaults.DragHandle(Modifier.align(Alignment.CenterHorizontally)); body() }
                }
            }
        }
        return
    }
    ModalBottomSheet(onDismissRequest = onDismiss, sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = true),
        shape = RelayTokens.sheet, containerColor = MaterialTheme.colorScheme.surfaceContainerHigh,
        dragHandle = { BottomSheetDefaults.DragHandle() }) {
        body()
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

@Composable fun StepSheet(step: PathStep, onAction: (StepAction) -> Unit, onRecheck: () -> Unit, onDismiss: () -> Unit = {}) {
    var visible by remember(step.id) { mutableStateOf(true) }
    if (!visible) return
    RelaySheet({ visible = false; onDismiss() }) {
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
            StatusGlyph(step.state, 40.dp, stateKey = step.id to step.sinceMillis)
            IconTile(stepIcon(step.id), stepHue(step.id))
            Text(stringResource(step.title), Modifier.weight(1f).semantics { heading() }, style = RelayTokens.title)
        }
        step.detail?.let { StepDetail(step, style = RelayTokens.body) }
        step.explanation?.let { Text(stringResource(it), style = RelayTokens.callout, color = MaterialTheme.colorScheme.onSurfaceVariant) }
        step.evidence?.let { EvidenceRow(stepText(it)) }
        step.code?.let { EvidenceRow(it) }
        step.sinceMillis?.let {
            val time = remember(it) { DateFormat.getDateTimeInstance(DateFormat.SHORT, DateFormat.MEDIUM).format(Date(it)) }
            Text(time, style = RelayTokens.mono, color = MaterialTheme.colorScheme.onSurfaceVariant)
        }
        val addresses = if (step.id == StepId.CAR_PAGE) step.trail.filter { it.startsWith("https://") }.distinct() else emptyList()
        addresses.filter { it != step.evidence?.raw }.forEach { EvidenceRow(it) }
        val trail = step.trail - addresses.toSet()
        if (trail.isNotEmpty()) {
            Text(stringResource(R.string.design_trail), Modifier.semantics { heading() }, style = RelayTokens.headline)
            SelectionContainer { Column(Modifier.fillMaxWidth().heightIn(max = 200.dp).verticalScroll(rememberScrollState())) {
                trail.forEach { Text(it, style = if (evidenceUsesMonospace(it)) RelayTokens.mono else RelayTokens.caption) }
            } }
            EvidenceRow(trail.joinToString("\n"))
        }
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            step.action?.let { action -> Button({ onAction(action) }, Modifier.weight(1f).heightIn(min = 48.dp), shape = RelayTokens.pill) {
                Text(stringResource(step.actionLabel ?: R.string.design_open), textAlign = TextAlign.Center)
            } }
            TextButton(onRecheck, Modifier.heightIn(min = 48.dp)) { Text(stringResource(R.string.design_check_again), textAlign = TextAlign.Center) }
        }
    }
}
