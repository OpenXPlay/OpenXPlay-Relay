// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-14
package org.openxplay.direlay.ui

import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.ui.text.font.FontWeight

val RelayGreen = Color(0xFF54E8A0)
@Composable fun RelayTheme(content: @Composable () -> Unit) {
    MaterialTheme(colorScheme = darkColorScheme(primary = RelayGreen, onPrimary = Color(0xFF080808),
        primaryContainer = Color(0xFF1E1E1E), onPrimaryContainer = RelayGreen,
        secondary = RelayGreen, onSecondary = Color(0xFF080808),
        secondaryContainer = Color(0xFF1E1E1E), onSecondaryContainer = RelayGreen,
        surfaceTint = Color.Transparent, outlineVariant = Color(0xFF2C2C2C),
        background = Color(0xFF080808), surface = Color(0xFF151515), surfaceVariant = Color(0xFF1E1E1E),
        onSurface = Color.White, onSurfaceVariant = Color(0xFFA8A8A8), outline = Color(0xFF2C2C2C),
        error = Color(0xFFFF5C5C)), shapes = Shapes(extraSmall = RoundedCornerShape(12.dp),
        small = RoundedCornerShape(12.dp), medium = RoundedCornerShape(16.dp),
        large = RoundedCornerShape(16.dp)), content = content)
}
@Composable fun RelayPage(content: @Composable ColumnScope.() -> Unit) {
    Surface(Modifier.fillMaxSize(), color = MaterialTheme.colorScheme.background) {
        Column(Modifier.fillMaxSize().safeDrawingPadding().imePadding().verticalScroll(rememberScrollState())
            .padding(20.dp), verticalArrangement = Arrangement.spacedBy(16.dp), content = content)
    }
}
@Composable fun RelayCard(content: @Composable ColumnScope.() -> Unit) {
    Card(Modifier.fillMaxWidth(), shape = RoundedCornerShape(16.dp),
        colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surface)) {
        Column(Modifier.padding(20.dp), verticalArrangement = Arrangement.spacedBy(12.dp), content = content)
    }
}
@Composable fun Heading(text: String) = Text(text, modifier = Modifier.semantics { heading() }, fontSize = 23.sp, fontWeight = FontWeight.SemiBold)
@Composable fun Hint(text: String, enabled: Boolean = true) = Text(text,
    color = if (enabled) MaterialTheme.colorScheme.onSurfaceVariant else MaterialTheme.colorScheme.onSurface.copy(alpha = .38f))
@Composable fun Toggle(label: String, value: Boolean, onChange: (Boolean) -> Unit) = Toggle(label, value, true, onChange)
@Composable fun Toggle(label: String, value: Boolean, enabled: Boolean, onChange: (Boolean) -> Unit) {
    Row(Modifier.fillMaxWidth().heightIn(min = 56.dp), horizontalArrangement = Arrangement.SpaceBetween,
        verticalAlignment = androidx.compose.ui.Alignment.CenterVertically) {
        Text(label, Modifier.weight(1f).padding(end = 12.dp),
            color = if (enabled) MaterialTheme.colorScheme.onSurface else MaterialTheme.colorScheme.onSurface.copy(alpha = .38f))
        LabeledSwitch(label, value, enabled, onChange)
    }
}

@Composable fun LabeledSwitch(label: String, value: Boolean, onChange: (Boolean) -> Unit) = LabeledSwitch(label, value, true, onChange)
@Composable fun LabeledSwitch(label: String, value: Boolean, enabled: Boolean, onChange: (Boolean) -> Unit) {
    Switch(value, onChange, Modifier.semantics { contentDescription = label }, enabled = enabled)
}
