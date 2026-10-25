// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-24
package org.openxplay.direlay.ui

import androidx.compose.foundation.layout.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import androidx.compose.ui.platform.LocalHapticFeedback
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import org.openxplay.direlay.R
import org.openxplay.direlay.path.*

@Composable internal fun HostOverlayScreen(value: HostOverlayPresentation, busy: Boolean,
    onStepAction: (StepAction, PathStep) -> Unit, onRecheck: () -> Unit, onCopy: (String) -> Unit,
    onAction: (HostOverlayAction) -> Unit) {
    val motion = rememberRelayMotion()
    val haptic = LocalHapticFeedback.current
    CompositionLocalProvider(LocalRelayMotion provides motion) {
        ConnectionPathPage(value.path, value.receiverUrls, value.problems, onStepAction, onRecheck, onCopy,
            onStop = { onAction(HostOverlayAction.STOP) }, actions = {
                Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    value.buttons.forEach { button ->
                        OutlinedButton({
                            if (button.action != HostOverlayAction.BACK) haptic.performHapticFeedback(HapticFeedbackType.LongPress)
                            onAction(button.action)
                        }, Modifier.weight(1f).heightIn(min = 56.dp), enabled = !busy,
                            contentPadding = PaddingValues(horizontal = 8.dp)) { Text(stringResource(button.label), textAlign = TextAlign.Center) }
                    }
                }
            })
    }
}
