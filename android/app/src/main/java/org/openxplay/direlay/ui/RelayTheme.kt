// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-14
package org.openxplay.direlay.ui

import androidx.compose.foundation.layout.*
import androidx.compose.foundation.selection.toggleable
import androidx.compose.ui.semantics.Role
import org.openxplay.direlay.ui.design.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.dp

val RelayGreen: Color @Composable get() = MaterialTheme.colorScheme.primary
@Composable fun RelayTheme(content: @Composable () -> Unit) = RelayDesignTheme(content = content)
@Composable fun RelayPage(content: @Composable ColumnScope.() -> Unit) {
    Surface(Modifier.fillMaxSize(), color = MaterialTheme.colorScheme.background) {
        Box(Modifier.fillMaxSize().safeDrawingPadding().imePadding(), contentAlignment = androidx.compose.ui.Alignment.TopCenter) {
            Column(Modifier.widthIn(max = 720.dp).fillMaxWidth().verticalScroll(rememberScrollState())
                .padding(16.dp), verticalArrangement = Arrangement.spacedBy(16.dp), content = content)
        }
    }
}
@Composable fun Heading(text: String) = Text(text, modifier = Modifier.semantics { heading() }, style = RelayTokens.title)
@Composable fun Hint(text: String, enabled: Boolean = true) = Text(text,
    style = RelayTokens.footnote,
    color = if (enabled) MaterialTheme.colorScheme.onSurfaceVariant else MaterialTheme.colorScheme.onSurface.copy(alpha = .38f))
@Composable fun Toggle(label: String, value: Boolean, onChange: (Boolean) -> Unit) = Toggle(label, value, true, onChange)
@Composable fun Toggle(label: String, value: Boolean, enabled: Boolean, onChange: (Boolean) -> Unit) {
    Row(Modifier.fillMaxWidth().heightIn(min = 56.dp)
        .toggleable(value, enabled = enabled, role = Role.Switch, onValueChange = onChange), horizontalArrangement = Arrangement.SpaceBetween,
        verticalAlignment = androidx.compose.ui.Alignment.CenterVertically) {
        Text(label, Modifier.weight(1f).padding(end = 12.dp),
            color = if (enabled) MaterialTheme.colorScheme.onSurface else MaterialTheme.colorScheme.onSurface.copy(alpha = .38f))
        RelaySwitch(value, null, enabled = enabled)
    }
}

@Composable fun LabeledSwitch(label: String, value: Boolean, onChange: (Boolean) -> Unit) = LabeledSwitch(label, value, true, onChange)
@Composable fun LabeledSwitch(label: String, value: Boolean, enabled: Boolean, onChange: (Boolean) -> Unit) {
    RelaySwitch(value, onChange, label, enabled)
}
