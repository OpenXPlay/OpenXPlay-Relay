// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-25
package org.openxplay.direlay.ui.design

import androidx.compose.material3.Text
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.TextStyle
import kotlinx.coroutines.delay
import org.openxplay.direlay.path.*

@Composable internal fun StepDetail(step: PathStep, modifier: Modifier = Modifier,
    style: TextStyle = RelayTokens.body, color: Color = Color.Unspecified) {
    val detail = step.detail ?: return
    val deadline = step.carWaitUntilMillis
    val value = if (deadline == null) stepText(detail) else {
        val seconds by produceState(countdownSeconds(deadline, System.currentTimeMillis()), deadline) {
            while (true) { value = countdownSeconds(deadline, System.currentTimeMillis()); delay(1000) }
        }
        detail.res?.let { stringResource(it, seconds.toString()) }.orEmpty()
    }
    Text(value, modifier, style = style, color = color)
}
