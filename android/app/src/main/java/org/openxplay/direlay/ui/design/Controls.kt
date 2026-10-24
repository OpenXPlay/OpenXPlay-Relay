// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-24
package org.openxplay.direlay.ui.design

import androidx.compose.animation.animateColorAsState
import androidx.compose.animation.animateContentSize
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
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import org.openxplay.direlay.R
import org.openxplay.direlay.path.PathTone
import org.openxplay.direlay.ui.LocalRelayMotion
import org.openxplay.direlay.ui.Symbol

@Composable fun PrimaryPill(running: Boolean, enabled: Boolean, label: String, onClick: () -> Unit) {
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
            modifier = Modifier.fillMaxWidth(width).height(56.dp).graphicsLayer { scaleX = scale; scaleY = scale },
            colors = ButtonDefaults.buttonColors(containerColor = color,
                contentColor = if (running && LocalRelayDark.current) RelayTokens.darkScheme.onPrimary else MaterialTheme.colorScheme.onPrimary)) {
            Symbol(if (running) R.drawable.symbol_square else R.drawable.symbol_power, null, Modifier.size(20.dp))
            Spacer(Modifier.width(8.dp))
            Text(label, style = RelayTokens.headline, maxLines = 1, overflow = TextOverflow.Ellipsis)
        }
    }
}

@Composable fun AutoSwitch(checked: Boolean, onChange: (Boolean) -> Unit) {
    val label = stringResource(R.string.design_auto)
    Row(Modifier.heightIn(min = 48.dp).toggleable(checked, role = Role.Switch, onValueChange = onChange)
        .semantics(mergeDescendants = true) { contentDescription = label },
        verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(4.dp)) {
        Text(label, style = RelayTokens.footnote)
        RelaySwitch(checked, null)
    }
}

@Composable fun StatusCapsule(tone: PathTone, text: String) {
    val motion = LocalRelayMotion.current
    val color = LocalStatusColors.current.color(tone)
    val breath = rememberRelayBreath(tone == PathTone.PROGRESS)
    val state = stringResource(when (tone) {
        PathTone.LIVE -> R.string.design_live
        PathTone.READY -> R.string.design_ready
        PathTone.PROGRESS -> R.string.design_state_working
        PathTone.ACTION -> R.string.design_state_action
        PathTone.ERROR -> R.string.design_state_failed
        PathTone.IDLE -> R.string.design_idle
    })
    Surface(shape = RelayTokens.pill, color = color.copy(alpha = .14f),
        modifier = Modifier.widthIn(max = 240.dp).animateContentSize(relaySpatial(motion)).semantics(mergeDescendants = true) { stateDescription = state }) {
        Row(Modifier.padding(horizontal = 12.dp, vertical = 8.dp), verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            Box(Modifier.size(8.dp).graphicsLayer { alpha = if (tone == PathTone.PROGRESS) (breath.value - .84f) / .16f else 1f }
                .background(color, RelayTokens.pill))
            Text(text, style = RelayTokens.caption.copy(fontFeatureSettings = "tnum"), maxLines = 1, overflow = TextOverflow.Ellipsis)
        }
    }
}

@Composable fun DiagnosticLine(text: String, onCopy: (String) -> Unit) {
    val haptic = LocalHapticFeedback.current
    val label = stringResource(R.string.design_copy_diagnostic)
    SelectionContainer {
        Text(text, Modifier.fillMaxWidth().heightIn(min = 48.dp).semantics { onClick(label) {
            haptic.performHapticFeedback(HapticFeedbackType.TextHandleMove); onCopy(text); true
        } }.clickable(onClickLabel = label) {
            haptic.performHapticFeedback(HapticFeedbackType.TextHandleMove); onCopy(text)
        }.padding(vertical = 8.dp), style = RelayTokens.mono, color = MaterialTheme.colorScheme.onSurfaceVariant)
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
