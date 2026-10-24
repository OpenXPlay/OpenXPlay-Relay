// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-24
package org.openxplay.direlay.ui.design

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.selection.toggleable
import androidx.compose.material3.*
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.*
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import org.openxplay.direlay.R
import org.openxplay.direlay.ui.Symbol

@Composable fun IconTile(icon: Int, hue: IconHue, modifier: Modifier = Modifier) {
    Surface(modifier.size(32.dp), shape = RelayTokens.tile, color = iconHue(hue), contentColor = RelayTokens.tileInk) {
        Box(contentAlignment = Alignment.Center) { Symbol(icon, null, Modifier.size(20.dp)) }
    }
}

@Composable fun InsetGroup(header: String? = null, footer: String? = null, content: @Composable ColumnScope.() -> Unit) {
    Column(Modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(8.dp)) {
        header?.let { Text(it, Modifier.padding(horizontal = 16.dp).semantics { heading() }, style = RelayTokens.footnote,
            color = MaterialTheme.colorScheme.onSurfaceVariant) }
        Surface(shape = RelayTokens.group, color = MaterialTheme.colorScheme.surface,
            border = if (LocalRelayDark.current) null else BorderStroke(1.dp, MaterialTheme.colorScheme.outlineVariant)) {
            Column(Modifier.fillMaxWidth(), content = content)
        }
        footer?.let { Text(it, Modifier.padding(horizontal = 16.dp), style = RelayTokens.footnote,
            color = MaterialTheme.colorScheme.onSurfaceVariant) }
    }
}

@Composable fun InsetRow(icon: Int? = null, tile: IconHue = IconHue.DEVELOPER, title: String,
    value: String? = null, onClick: (() -> Unit)? = null, trailing: @Composable (() -> Unit)? = null) {
    Row(Modifier.fillMaxWidth().heightIn(min = 56.dp)
        .then(if (onClick == null) Modifier else Modifier.clickable(role = Role.Button, onClick = onClick))
        .padding(horizontal = 16.dp, vertical = 8.dp), verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(12.dp)) {
        if (icon != null) IconTile(icon, tile)
        Text(title, Modifier.weight(1f), style = RelayTokens.body, maxLines = 2, overflow = TextOverflow.Ellipsis)
        if (value != null) Text(value, Modifier.weight(.8f, fill = false), style = RelayTokens.callout,
            color = MaterialTheme.colorScheme.onSurfaceVariant, maxLines = 1, overflow = TextOverflow.Ellipsis)
        if (trailing != null) trailing() else if (onClick != null) Symbol(R.drawable.symbol_chevron, null, Modifier.size(16.dp))
    }
}

@Composable fun InsetToggleRow(icon: Int? = null, tile: IconHue = IconHue.DEVELOPER, title: String,
    checked: Boolean, onChange: (Boolean) -> Unit, enabled: Boolean = true) {
    val state = stringResource(if (checked) R.string.menu_on else R.string.menu_off)
    Row(Modifier.fillMaxWidth().heightIn(min = 56.dp).semantics { stateDescription = state }
        .toggleable(checked, enabled = enabled, role = Role.Switch, onValueChange = onChange)
        .padding(horizontal = 16.dp, vertical = 8.dp), verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(12.dp)) {
        if (icon != null) IconTile(icon, tile)
        Text(title, Modifier.weight(1f), style = RelayTokens.body, maxLines = 2, overflow = TextOverflow.Ellipsis)
        RelaySwitch(checked, null, enabled = enabled)
    }
}

@Composable fun InsetDivider() = HorizontalDivider(Modifier.padding(start = 60.dp), thickness = 1.dp,
    color = MaterialTheme.colorScheme.outlineVariant)
