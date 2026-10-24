// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-24
package org.openxplay.direlay.ui.design

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.expandVertically
import androidx.compose.animation.shrinkVertically
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.tween
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.rotate
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.*
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import org.openxplay.direlay.R
import org.openxplay.direlay.path.*
import org.openxplay.direlay.ui.LocalRelayMotion
import org.openxplay.direlay.ui.Symbol

internal fun sectionTitle(section: PathSection) = when (section) {
    PathSection.RELAY -> R.string.design_section_relay
    PathSection.CAR -> R.string.design_section_car
    PathSection.IPHONE -> R.string.design_section_iphone
    PathSection.PICTURE -> R.string.design_section_picture
}
internal fun sectionNumber(section: PathSection) = when (section) {
    PathSection.RELAY -> "①"
    PathSection.CAR -> "②"
    PathSection.IPHONE -> "③"
    PathSection.PICTURE -> "④"
}

@Composable fun PathSectionCard(section: PathSectionView, expanded: Boolean, onExpand: (Boolean) -> Unit,
    onStep: (PathStep) -> Unit, onAction: (StepAction) -> Unit, focusedStep: StepId? = null, autoExpand: Boolean = true, pulse: Int = 0,
    onStepAction: ((StepAction, PathStep) -> Unit)? = null) {
    val motion = LocalRelayMotion.current
    val allOk = section.state == StepState.OK
    // Preserve manual disclosure across rotation; a changed step state reveals new work.
    val stateKey = section.steps.joinToString { "${it.id}:${it.state}" }
    var previousState by rememberSaveable(section.section.name) { mutableStateOf<String?>(null) }
    LaunchedEffect(stateKey) {
        if (previousState != stateKey) { if (autoExpand) onExpand(!allOk); previousState = stateKey }
    }
    val disclosure by animateFloatAsState(if (expanded) 180f else 0f, relaySpatial(motion), label = "section disclosure")
    val state = stringResource(stateLabel(section.state))
    val expansion = stringResource(if (expanded) R.string.menu_expanded else R.string.menu_collapsed)
    val evidenceIds = when (section.section) {
        PathSection.RELAY -> setOf(StepId.BLUETOOTH, StepId.HOTSPOT, StepId.SHARED_WIFI)
        PathSection.CAR -> setOf(StepId.CAR_PAGE, StepId.CARLINK_READY, StepId.DLNA, StepId.MIRACAST)
        PathSection.IPHONE -> setOf(StepId.IPHONE_PAIRED, StepId.SESSION)
        PathSection.PICTURE -> setOf(StepId.TESLA, StepId.PICTURE)
    }
    val keySteps = section.steps.filter { it.id in evidenceIds && it.evidence != null }.ifEmpty { section.steps }
    val evidence = keySteps.mapNotNull { it.evidence?.let { text -> stepText(text) } }.distinct().joinToString(" · ")
    Card(Modifier.fillMaxWidth(), shape = RelayTokens.group,
        border = if (LocalRelayDark.current) null else BorderStroke(1.dp, MaterialTheme.colorScheme.outlineVariant),
        colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surface)) {
        Row(Modifier.fillMaxWidth().heightIn(min = 56.dp).semantics(mergeDescendants = true) {
            heading(); stateDescription = "$state, $expansion"
        }.clickable(role = Role.Button) { onExpand(!expanded) }.padding(horizontal = 12.dp, vertical = 8.dp),
            verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            Text(sectionNumber(section.section), style = RelayTokens.headline, color = LocalStatusColors.current.color(section.state))
            Text(stringResource(sectionTitle(section.section)), Modifier.weight(1f), style = RelayTokens.headline,
                maxLines = 1, overflow = TextOverflow.Ellipsis)
            if (allOk && !expanded && evidence.isNotEmpty()) Text(evidence, Modifier.weight(1f),
                style = RelayTokens.mono, maxLines = 1, overflow = TextOverflow.Ellipsis, color = MaterialTheme.colorScheme.onSurfaceVariant)
            Text("${section.done}/${section.total}", style = RelayTokens.mono, color = MaterialTheme.colorScheme.onSurfaceVariant)
            StatusGlyph(section.state, 24.dp, announce = false)
            Symbol(R.drawable.symbol_chevron_down, null, Modifier.size(16.dp).rotate(disclosure))
        }
        AnimatedVisibility(expanded, enter = expandVertically(relaySpatial(motion)), exit = shrinkVertically(relaySpatial(motion))) {
            Column {
                section.steps.forEachIndexed { index, step ->
                    key(step.id) {
                        if (index > 0) InsetDivider()
                        val emphasis = remember { Animatable(1f) }
                        LaunchedEffect(pulse, focusedStep) {
                            if (pulse > 0 && step.id == focusedStep && motion) {
                                repeat(2) { emphasis.animateTo(.45f, tween(140)); emphasis.animateTo(1f, tween(180)) }
                            } else emphasis.snapTo(1f)
                        }
                        Box(Modifier.graphicsLayer { alpha = emphasis.value }) {
                            StepRow(step, { onStep(step) }, { action ->
                                if (onStepAction != null) onStepAction(action, step) else onAction(action)
                            }, focused = step.id == focusedStep)
                        }
                    }
                }
            }
        }
    }
}
