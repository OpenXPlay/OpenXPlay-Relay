// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-24
package org.openxplay.direlay.ui.design

import androidx.compose.animation.animateColorAsState
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.foundation.background
import androidx.compose.foundation.selection.toggleable
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.draw.alpha
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.collectIsPressedAsState
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import androidx.compose.ui.platform.LocalHapticFeedback
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.*
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import org.openxplay.direlay.R
import org.openxplay.direlay.path.PathSection
import org.openxplay.direlay.path.StepState
import org.openxplay.direlay.ui.LocalRelayMotion
import org.openxplay.direlay.ui.Symbol

/** [icon]: null shows the label alone (e.g. "Next"); the default is the start/stop symbol. */
@Composable fun PrimaryPill(running: Boolean, enabled: Boolean, label: String,
    icon: Int? = if (running) R.drawable.symbol_square else R.drawable.symbol_power, onClick: () -> Unit) {
    val motion = LocalRelayMotion.current
    val colors = LocalStatusColors.current
    val color by animateColorAsState(if (running) colors.action else colors.ok, relayEffects(motion), label = "primary color")
    val width by animateFloatAsState(if (running) .9f else 1f, relaySpatial(motion), label = "primary width")
    val interactions = remember { MutableInteractionSource() }
    val pressed by interactions.collectIsPressedAsState()
    val scale by animateFloatAsState(if (pressed) .97f else 1f, relaySpatial(motion), label = "primary press")
    val haptic = LocalHapticFeedback.current
    Box(Modifier.fillMaxWidth(), contentAlignment = Alignment.Center) {
        Button(onClick = { haptic.performHapticFeedback(HapticFeedbackType.LongPress); onClick() }, enabled = enabled,
            interactionSource = interactions, shape = RelayTokens.pill,
            modifier = Modifier.fillMaxWidth(width).heightIn(min = 56.dp).graphicsLayer { scaleX = scale; scaleY = scale },
            colors = ButtonDefaults.buttonColors(containerColor = color,
                contentColor = if (running && LocalRelayDark.current) RelayTokens.darkScheme.onPrimary else MaterialTheme.colorScheme.onPrimary)) {
            if (icon != null) {
                Symbol(icon, null, Modifier.size(20.dp))
                Spacer(Modifier.width(8.dp))
            }
            Text(label, Modifier.weight(1f, fill = false), style = RelayTokens.headline, textAlign = TextAlign.Center)
        }
    }
}

/** Keep each code with its glyph (word joiner, no visible gap) and each section letter with its first token. */
internal fun diagnosticSections(text: String): List<String> = text.split(" | ").map { section ->
    val tokens = section.split(' ')
    val pathSection = PathSection.entries.any { it.code == tokens.first() }
    val display = tokens.mapIndexed { index, token ->
        if (pathSection && index > 0 && StepState.entries.any { it.glyph == token.lastOrNull() })
            token.dropLast(1) + "\u2060" + token.last()
        else token
    }.joinToString(" ")
    if (pathSection || tokens.first() == "E") display.replaceFirst(" ", "\u00A0") else display
}

@OptIn(ExperimentalLayoutApi::class)
@Composable fun DiagnosticLine(line: String, onCopy: (String) -> Unit, version: String = "", active: Boolean = true) {
    val now by produceState(System.currentTimeMillis(), line, version, active) {
        if (active) while (true) { value = System.currentTimeMillis(); kotlinx.coroutines.delay(1000) }
    }
    val text = org.openxplay.direlay.path.formatDiagnostic(line, version, now)
    val haptic = LocalHapticFeedback.current
    val label = stringResource(R.string.design_copy_diagnostic)
    val sections = remember(text) { diagnosticSections(text) }
    SelectionContainer {
        // Prefer a new line between sections; long sections wrap between their tokens.
        FlowRow(Modifier.fillMaxWidth().heightIn(min = 48.dp).semantics { onClick(label) {
            haptic.performHapticFeedback(HapticFeedbackType.TextHandleMove); onCopy(text); true
        } }.clickable(onClickLabel = label) {
            haptic.performHapticFeedback(HapticFeedbackType.TextHandleMove); onCopy(text)
        }.padding(vertical = 8.dp), horizontalArrangement = Arrangement.spacedBy(6.dp)) {
            sections.forEachIndexed { index, section ->
                Text(section + if (index < sections.lastIndex) "\u00A0|" else "", style = RelayTokens.mono,
                    color = MaterialTheme.colorScheme.onSurfaceVariant, softWrap = true)
            }
        }
    }
}

/** A switch with the same motion policy as the path, including local motion overrides. */
@Composable fun RelaySwitch(checked: Boolean, onChange: ((Boolean) -> Unit)?, label: String = "", enabled: Boolean = true) {
    val motion = LocalRelayMotion.current
    val position by animateFloatAsState(if (checked) 1f else 0f, relaySpatial(motion), label = "switch thumb")
    val track by animateColorAsState(if (checked) MaterialTheme.colorScheme.primary else MaterialTheme.colorScheme.outlineVariant,
        relayEffects(motion), label = "switch track")
    val travel = with(LocalDensity.current) { 20.dp.toPx() }
    val state = stringResource(if (checked) R.string.menu_on else R.string.menu_off)
    Box(Modifier.size(52.dp, 48.dp).alpha(if (enabled) 1f else .38f)
        .then(if (onChange == null) Modifier else Modifier.toggleable(checked, enabled = enabled, role = Role.Switch, onValueChange = onChange)
            .semantics { if (label.isNotEmpty()) contentDescription = label; stateDescription = state }), contentAlignment = Alignment.CenterStart) {
        Box(Modifier.fillMaxWidth().height(32.dp).background(track, RelayTokens.pill))
        Box(Modifier.padding(start = 4.dp).size(24.dp).graphicsLayer { translationX = position * travel }
            .background(RelayTokens.tileInk, RelayTokens.pill))
    }
}
