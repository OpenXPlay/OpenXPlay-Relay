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
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.layout.Layout
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.*
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.Constraints
import androidx.compose.ui.unit.constrainHeight
import androidx.compose.ui.unit.constrainWidth
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
        footer?.let { Text(it, Modifier.padding(start = 16.dp, end = 16.dp, bottom = 8.dp), style = RelayTokens.footnote,
            color = MaterialTheme.colorScheme.onSurfaceVariant) }
    }
}

/** True when title and value both fit on one line, whole, beside each other. */
internal fun valueFitsInline(titleWidth: Int, valueWidth: Int, gap: Int, available: Int): Boolean =
    titleWidth.toLong() + valueWidth + gap <= available

internal fun summarySegments(value: String): List<String> = value.split(" · ").filter { it.isNotBlank() }.let { segments ->
    segments.mapIndexed { index, segment -> segment + if (index < segments.lastIndex) " ·" else "" }
}

@Composable internal fun SegmentedText(value: String, style: TextStyle, color: Color, modifier: Modifier = Modifier) =
    SegmentedText(AnnotatedString(value), style, color, modifier)

@OptIn(ExperimentalLayoutApi::class)
@Composable internal fun SegmentedText(value: AnnotatedString, style: TextStyle, color: Color, modifier: Modifier = Modifier) {
    FlowRow(modifier.semantics(mergeDescendants = true) { contentDescription = value.text },
        horizontalArrangement = Arrangement.spacedBy(4.dp), verticalArrangement = Arrangement.spacedBy(0.dp)) {
        var start = 0
        summarySegments(value.text).forEach { segment ->
            val offset = value.text.indexOf(segment, start)
            Text(value.subSequence(offset, offset + segment.length), Modifier.clearAndSetSemantics {}, style = style, color = color)
            start = offset + segment.length
        }
    }
}

@Composable internal fun TitleValue(title: String, value: String, modifier: Modifier = Modifier) {
    Layout(content = {
        Text(title, style = RelayTokens.body)
        // Preserve the plain text's intrinsic width and inline layout when deciding whether to stack.
        Text(value, style = RelayTokens.callout, textAlign = TextAlign.Start, color = MaterialTheme.colorScheme.onSurfaceVariant)
        SegmentedText(value, style = RelayTokens.callout, color = MaterialTheme.colorScheme.onSurfaceVariant)
    }, modifier = modifier) { measurables, constraints ->
        val titleWidth = measurables[0].maxIntrinsicWidth(Constraints.Infinity)
        val valueWidth = measurables[1].maxIntrinsicWidth(Constraints.Infinity)
        val gap = 12.dp.roundToPx()
        val width = if (constraints.hasBoundedWidth) constraints.maxWidth else
            constraints.constrainWidth((titleWidth.toLong() + valueWidth + gap).coerceAtMost(Int.MAX_VALUE.toLong()).toInt())
        val inline = '\n' !in title && '\n' !in value && valueFitsInline(titleWidth, valueWidth, gap, width)
        val titlePlaceable = measurables[0].measure(Constraints(maxWidth = if (inline) titleWidth else width))
        val valuePlaceable = measurables[if (inline) 1 else 2].measure(Constraints(maxWidth = if (inline) valueWidth else width))
        val spacing = if (inline) 0 else 2.dp.roundToPx()
        val height = constraints.constrainHeight(if (inline) maxOf(titlePlaceable.height, valuePlaceable.height) else
            titlePlaceable.height + spacing + valuePlaceable.height)
        layout(width, height) {
            titlePlaceable.placeRelative(0, if (inline) (height - titlePlaceable.height) / 2 else 0)
            valuePlaceable.placeRelative(if (inline) width - valuePlaceable.width else 0,
                if (inline) (height - valuePlaceable.height) / 2 else titlePlaceable.height + spacing)
        }
    }
}

@Composable fun InsetRow(icon: Int? = null, tile: IconHue = IconHue.DEVELOPER, title: String,
    value: String? = null, onClick: (() -> Unit)? = null, trailing: @Composable (() -> Unit)? = null) {
    Row(Modifier.fillMaxWidth().heightIn(min = 56.dp)
        .then(if (onClick == null) Modifier else Modifier.clickable(role = Role.Button, onClick = onClick))
        .padding(horizontal = 16.dp, vertical = 8.dp), verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(12.dp)) {
        if (icon != null) IconTile(icon, tile)
        if (value == null) Text(title, Modifier.weight(1f), style = RelayTokens.body)
        else TitleValue(title, value, Modifier.weight(1f))
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
        Text(title, Modifier.weight(1f), style = RelayTokens.body)
        RelaySwitch(checked, null, enabled = enabled)
    }
}

@Composable fun InsetDivider() = HorizontalDivider(Modifier.padding(start = 60.dp), thickness = 1.dp,
    color = MaterialTheme.colorScheme.outlineVariant)
