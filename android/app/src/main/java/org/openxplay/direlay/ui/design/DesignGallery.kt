// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-24
package org.openxplay.direlay.ui.design

import android.widget.Toast
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.foundation.layout.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.ui.Modifier
import android.content.ClipData
import android.content.ClipboardManager
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import org.openxplay.direlay.R
import org.openxplay.direlay.path.*
import org.openxplay.direlay.ui.LocalRelayMotion

@OptIn(ExperimentalLayoutApi::class)
@Composable fun DesignGalleryScreen(onBack: () -> Unit) {
    val systemDark = isSystemInDarkTheme()
    var dark by rememberSaveable { mutableStateOf(systemDark) }
    var motion by rememberSaveable { mutableStateOf(true) }
    var selected by rememberSaveable { mutableIntStateOf(0) }
    var automatic by rememberSaveable { mutableStateOf(false) }
    var sheetStep by remember { mutableStateOf<PathStep?>(null) }
    var showSheet by rememberSaveable { mutableStateOf(false) }
    val pagesLabel = stringResource(R.string.design_pages, 1)
    val samples = remember(pagesLabel) { samplePaths(System.currentTimeMillis(), pagesLabel) }
    val sample = samples[selected]
    val context = LocalContext.current
    val clipboard = context.getSystemService(ClipboardManager::class.java)
    val copied = stringResource(R.string.design_copied)
    val preview = stringResource(R.string.design_preview_action)
    val copy: (String) -> Unit = { clipboard.setPrimaryClip(ClipData.newPlainText(copied, it)); Toast.makeText(context, copied, Toast.LENGTH_SHORT).show() }
    val action: (StepAction) -> Unit = { Toast.makeText(context, preview, Toast.LENGTH_SHORT).show() }
    val recheck: () -> Unit = { Toast.makeText(context, preview, Toast.LENGTH_SHORT).show() }
    CompositionLocalProvider(LocalRelayMotion provides (LocalRelayMotion.current && motion)) {
        RelayDesignTheme(dark) {
            LargeTitleScaffold(stringResource(R.string.design_gallery), onBack) {
                InsetGroup(footer = stringResource(R.string.design_gallery_hint)) {
                    InsetToggleRow(R.drawable.symbol_smartphone, IconHue.IPHONE, stringResource(R.string.design_dark), dark, { dark = it })
                    InsetDivider()
                    InsetToggleRow(R.drawable.symbol_activity, IconHue.DEVELOPER, stringResource(R.string.design_motion), motion, { motion = it })
                }
                Text(stringResource(R.string.design_paths), style = RelayTokens.title)
                FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                    samples.forEachIndexed { index, item -> FilterChip(selected == index, { selected = index },
                        modifier = Modifier.heightIn(min = 48.dp), label = { Text(stringResource(item.title)) }) }
                }
                GalleryPathSample(sample, action, recheck, copy, { sheetStep = it })
                AutoSwitch(automatic) { automatic = it }
                Text(stringResource(R.string.design_states), style = RelayTokens.title)
                InsetGroup {
                    StepState.entries.forEachIndexed { index, state ->
                        if (index > 0) InsetDivider()
                        val step = PathStep(StepId.HOTSPOT, state, R.string.design_step_hotspot,
                            StepText.of(stateLabel(state)), if (state == StepState.OK) StepText.of(R.string.design_state_ok) else StepText.raw("OpenXPlay-1A2B"),
                            action = if (state == StepState.ACTION || state == StepState.FAILED) StepAction.OPEN_HOTSPOT else null,
                            actionLabel = R.string.design_open, sinceMillis = System.currentTimeMillis() - 12_000)
                        StepRow(step, { sheetStep = step }, action, focused = state == StepState.ACTION)
                    }
                }
                PathTone.entries.forEach { StatusCapsule(it, stringResource(when (it) {
                    PathTone.LIVE -> R.string.design_live
                    PathTone.READY -> R.string.design_ready
                    PathTone.PROGRESS -> R.string.design_sample_car_wait
                    PathTone.ACTION -> R.string.design_state_action
                    PathTone.ERROR -> R.string.design_state_failed
                    PathTone.IDLE -> R.string.design_idle
                })) }
                StepState.entries.forEach { state ->
                    val step = PathStep(StepId.HOTSPOT, state, R.string.design_step_hotspot, StepText.of(stateLabel(state)),
                        if (state == StepState.OK) StepText.of(R.string.design_state_ok) else StepText.raw("OpenXPlay-1A2B · 5 GHz"),
                        action = if (state == StepState.ACTION || state == StepState.FAILED) StepAction.OPEN_HOTSPOT else null,
                        actionLabel = R.string.design_open)
                    FocusCard(step, action, recheck, { sheetStep = step })
                }
                PrimaryPill(false, true, stringResource(R.string.design_connect), onClick = recheck)
                PrimaryPill(true, true, stringResource(R.string.design_stop), onClick = recheck)
                PrimaryPill(false, false, stringResource(R.string.design_connect), onClick = recheck)
                PrimaryPill(true, false, stringResource(R.string.design_stop), onClick = recheck)
                InsetGroup(stringResource(R.string.design_lists), stringResource(R.string.design_gallery_hint)) {
                    InsetRow(R.drawable.symbol_bluetooth, IconHue.BLUETOOTH, stringResource(R.string.design_step_bluetooth), "Redmi 14C", recheck)
                    InsetDivider()
                    InsetToggleRow(R.drawable.symbol_radio, IconHue.NETWORK, stringResource(R.string.design_step_hotspot), automatic, { automatic = it })
                    InsetDivider()
                    InsetToggleRow(R.drawable.symbol_shield_check, IconHue.PERMISSIONS, stringResource(R.string.design_step_permissions), false, {}, enabled = false)
                }
                GalleryLongText(samples.last().path, action, recheck, { sheetStep = it })
                Text(stringResource(R.string.design_icons), style = RelayTokens.title)
                FlowRow(horizontalArrangement = Arrangement.spacedBy(12.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
                    IconHue.entries.forEach { hue -> Column(horizontalAlignment = androidx.compose.ui.Alignment.CenterHorizontally) {
                        IconTile(R.drawable.symbol_settings_2, hue)
                        Text(stringResource(when (hue) {
                            IconHue.BLUETOOTH -> R.string.design_step_bluetooth
                            IconHue.NETWORK -> R.string.design_step_wifi
                            IconHue.CAR -> R.string.design_section_car
                            IconHue.IPHONE -> R.string.design_section_iphone
                            IconHue.IDENTITY -> R.string.design_step_identity
                            IconHue.PERMISSIONS -> R.string.design_step_permissions
                            IconHue.PICTURE -> R.string.design_section_picture
                            IconHue.LOGS -> R.string.design_logs
                            IconHue.DEVELOPER -> R.string.design_developer
                        }), style = RelayTokens.caption)
                    } }
                }
                InsetGroup {
                    StepId.entries.forEach { id -> InsetRow(stepIcon(id), stepHue(id), stringResource(sampleStepTitle(id)), onClick = {
                        sheetStep = PathStep(id, StepState.OK, sampleStepTitle(id))
                    }) }
                }
                Button({ showSheet = true }, Modifier.heightIn(min = 48.dp), shape = RelayTokens.pill) { Text(stringResource(R.string.design_open_sheet)) }
            }
            sheetStep?.let { StepSheet(it, action, recheck, { sheetStep = null }) }
            if (showSheet) RelaySheet({ showSheet = false }) {
                Text(stringResource(R.string.design_gallery), style = RelayTokens.title)
                Text(stringResource(R.string.design_gallery_hint), style = RelayTokens.body)
                Button({ showSheet = false }, Modifier.heightIn(min = 48.dp)) { Text(stringResource(R.string.design_close)) }
            }
        }
    }
}

@Composable private fun GalleryLongText(path: ConnectionPath, action: (StepAction) -> Unit, recheck: () -> Unit,
    showStep: (PathStep) -> Unit) {
    val step = remember { PathStep(StepId.HOTSPOT, StepState.ACTION, R.string.design_long_title,
        StepText.of(R.string.design_long_detail), StepText.raw("https://openxplay-relay-long-name.local:8443/receiver"),
        action = StepAction.OPEN_HOTSPOT, actionLabel = R.string.design_long_action) }
    Text(stringResource(R.string.design_long_text), style = RelayTokens.title)
    InsetGroup {
        InsetRow(R.drawable.symbol_radio, IconHue.NETWORK, stringResource(R.string.design_step_hotspot), "5 GHz", recheck)
        InsetDivider()
        InsetRow(R.drawable.symbol_globe, IconHue.NETWORK, stringResource(R.string.design_step_car_page),
            "https://openxplay-relay-long-name.local:8443/receiver", recheck)
        InsetDivider()
        InsetRow(R.drawable.symbol_monitor, IconHue.PICTURE, stringResource(R.string.design_long_title), "60 fps", recheck)
    }
    InsetGroup { StepRow(step, { showStep(step) }, action) }
    FocusCard(step, action, recheck, { showStep(step) })
    FocusCard(step, action, recheck, { showStep(step) }, compact = true)
    val iphone = "NGNkmmTRxZV67hI38XiRGPHTo"
    val longPath = path.copy(nodes = path.nodes.copy(iphone = iphone), sections = path.sections.map { section ->
        section.copy(steps = section.steps.map { if (it.id == StepId.IPHONE_PAIRED) it.copy(evidence = StepText.raw(iphone)) else it })
    })
    PathMap(longPath, onSection = { section -> longPath.section(section)?.steps?.firstOrNull()?.let(showStep) })
    PathMap(longPath, compact = true, onSection = { section -> longPath.section(section)?.steps?.firstOrNull()?.let(showStep) })
}

@Composable private fun GalleryPathSample(sample: DesignSample, action: (StepAction) -> Unit, recheck: () -> Unit,
    copy: (String) -> Unit, showStep: (PathStep) -> Unit) {
    val countdownStart = remember(sample.title) { System.currentTimeMillis() - 8000 }
    val path = if (sample.title == R.string.design_sample_car_wait) {
        val seconds = (20 - rememberElapsedSeconds(countdownStart)).coerceAtLeast(0)
        val focus = checkNotNull(sample.path.focus).copy(detail = StepText.of(R.string.design_car_countdown, seconds.toString()), sinceMillis = countdownStart)
        sample.path.copy(focus = focus, sections = sample.path.sections.map { section ->
            section.copy(steps = section.steps.map { if (it.id == focus.id) focus else it })
        })
    } else sample.path
    StatusCapsule(path.tone, stringResource(sample.title))
    PathMap(path, onSection = { section -> path.section(section)?.steps?.firstOrNull()?.let(showStep) })
    PathMap(path, compact = true, onSection = { section -> path.section(section)?.steps?.firstOrNull()?.let(showStep) })
    path.live?.let { LiveCard(it, path, recheck) }
        ?: path.focus?.let { FocusCard(it, action, recheck, { showStep(it) }) }
    path.sections.forEach { section ->
        key(sample.title, section.section) {
            var expanded by rememberSaveable { mutableStateOf(section.state != StepState.OK) }
            PathSectionCard(section, expanded, { expanded = it }, showStep, action, focusedStep = path.focus?.id)
        }
    }
    DiagnosticLine(path.diagnostic, copy)
    PrimaryPill(path.running, true, stringResource(if (path.running) R.string.design_stop else R.string.design_connect), onClick = recheck)
}
