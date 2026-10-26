// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-25
package org.openxplay.direlay.ui.design

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.flow.first
import org.openxplay.direlay.R
import org.openxplay.direlay.path.*
import org.openxplay.direlay.ui.*

@OptIn(ExperimentalLayoutApi::class)
@Composable fun StageGallery(samples: List<DesignSample>, onCopy: (String) -> Unit, onPreview: () -> Unit) {
    var selected by rememberSaveable { mutableIntStateOf(0) }
    var countdown by rememberSaveable { mutableStateOf(false) }
    var busy by rememberSaveable { mutableStateOf(false) }
    var notice by rememberSaveable { mutableIntStateOf(0) }
    var drawer by rememberSaveable { mutableStateOf(false) }
    Text(stringResource(R.string.stage_gallery), style = RelayTokens.title)
    FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
        samples.forEachIndexed { index, sample ->
            FilterChip(selected == index, { selected = index }, modifier = Modifier.heightIn(min = 48.dp),
                label = { Text(stringResource(sample.title)) })
        }
    }
    InsetGroup {
        InsetToggleRow(title = stringResource(R.string.stage_countdown), checked = countdown, onChange = { countdown = it })
        InsetDivider()
        InsetToggleRow(title = stringResource(R.string.stage_busy), checked = busy, onChange = { busy = it })
        InsetDivider()
        InsetToggleRow(title = stringResource(R.string.stage_drawer_open), checked = drawer, onChange = { drawer = it })
    }
    FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
        listOf(R.string.car_none, R.string.stage_notice_display, R.string.stage_notice_car, R.string.home2_setup).forEachIndexed { index, label ->
            FilterChip(notice == index, { notice = index }, modifier = Modifier.heightIn(min = 48.dp), label = { Text(stringResource(label)) })
        }
    }
    val sample = samples[selected.coerceIn(samples.indices)]
    val flags = StageFlags(countdown = countdown, busy = busy, displayChanged = notice == 1,
        carIncomplete = notice == 2, setupCompleted = notice != 3)
    val complete: () -> Unit = {
        if (countdown) { countdown = false; onPreview() }
    }
    StageGalleryFrame(sample, flags, drawer, 360.dp, 640.dp, onCopy, onPreview, complete)
    StageGalleryFrame(sample, flags, drawer, 760.dp, 340.dp, onCopy, onPreview, complete)
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable private fun StageGalleryFrame(sample: DesignSample, flags: StageFlags, drawerOpen: Boolean,
    width: Dp, height: Dp, onCopy: (String) -> Unit, onPreview: () -> Unit, onCountdownComplete: () -> Unit) {
    val state = rememberStagePageState()
    LaunchedEffect(drawerOpen) {
        if (drawerOpen) {
            snapshotFlow { state.scaffoldState.bottomSheetState.hasExpandedState }.first { it }
            state.expand()
        } else state.collapse()
    }
    var expanded by rememberSaveable(sample.title) { mutableStateOf(emptyList<String>()) }
    var fullyExpanded by rememberSaveable(sample.title) { mutableStateOf(emptyList<String>()) }
    val path = sample.path
    val mode = path.steps.firstOrNull { it.id == StepId.MODE }?.evidence?.let { stepText(it) }.orEmpty()
    val view = presentStage(path, flags)
    val keys: List<Any> = listOf("mode") + path.sections.map { it.section } + "diagnostic"
    Box(Modifier.fillMaxWidth().horizontalScroll(rememberScrollState())) {
        Surface(Modifier.requiredSize(width, height), border = BorderStroke(1.dp, MaterialTheme.colorScheme.outlineVariant)) {
            StagePage(path, view, onAction = { onPreview() }, onCopy = onCopy, onNotice = { onPreview() },
                onCountdownComplete = onCountdownComplete, state = state,
                topBar = {
                    Spacer(Modifier.weight(1f))
                    IconButton(onPreview, Modifier.size(48.dp)) {
                        Symbol(R.drawable.symbol_settings_2, stringResource(R.string.direlay_settings), Modifier.size(24.dp))
                    }
                }, onNode = { section ->
                    expanded = (expanded + section.name).distinct()
                    fullyExpanded = (fullyExpanded + section.name).distinct()
                }, drawerItemKeys = keys) {
                item(key = "mode") {
                    InsetRow(R.drawable.symbol_settings_2, IconHue.DEVELOPER, stringResource(R.string.setup2_set_up_again), mode, onPreview)
                }
                path.sections.forEach { section ->
                    item(key = section.section) {
                        PathSectionCard(section, section.section.name in expanded,
                            onExpand = { open -> expanded = if (open) (expanded + section.section.name).distinct() else expanded - section.section.name },
                            onStep = { onPreview() }, onAction = { onPreview() }, focusedStep = focusForPage(path)?.id,
                            autoExpand = section.section.name !in fullyExpanded,
                            fullyExpanded = true.takeIf { section.section.name in fullyExpanded })
                    }
                }
                item(key = "diagnostic") { DiagnosticLine(path.diagnostic, onCopy, path.version, active = state.isExpanded) }
            }
        }
    }
}
