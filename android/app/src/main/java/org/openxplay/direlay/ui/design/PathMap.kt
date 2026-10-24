// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-24
package org.openxplay.direlay.ui.design

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.compositeOver
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.*
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.Dp
import org.openxplay.direlay.R
import org.openxplay.direlay.path.*

@Composable fun PathMap(path: ConnectionPath, compact: Boolean = false, onSection: (PathSection) -> Unit) {
    val phase = rememberRelayPhase(path.tone == PathTone.LIVE)
    BoxWithConstraints(Modifier.fillMaxWidth().height(if (compact) 72.dp else 116.dp), contentAlignment = Alignment.Center) {
        val nodeWidth = ((maxWidth - 96.dp) / 3).coerceIn(48.dp, 72.dp)
        Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.Top) {
            PathNode(path.nodes.iphone ?: stringResource(R.string.design_section_iphone), R.drawable.symbol_smartphone,
                IconHue.IPHONE, null, path.section(PathSection.IPHONE), { onSection(PathSection.IPHONE) }, compact = compact, width = nodeWidth)
            PathLink(path.section(PathSection.IPHONE), "3", phase, 0, compact, Modifier.weight(1f)) { onSection(PathSection.IPHONE) }
            PathNode(path.nodes.relay ?: stringResource(R.string.design_section_relay), R.drawable.symbol_phone,
                IconHue.NETWORK, "1", path.section(PathSection.RELAY), { onSection(PathSection.RELAY) }, ring = true, compact = compact, width = nodeWidth)
            PathLink(path.section(PathSection.CAR), "2", phase, 1, compact, Modifier.weight(1f)) { onSection(PathSection.CAR) }
            PathNode(path.nodes.car ?: stringResource(R.string.design_section_car), R.drawable.symbol_car_front,
                IconHue.CAR, "4", path.section(PathSection.PICTURE), { onSection(PathSection.PICTURE) }, compact = compact, width = nodeWidth)
        }
    }
}

@Composable private fun PathBadge(number: String, state: StepState, modifier: Modifier = Modifier) {
    val colors = LocalStatusColors.current
    val background = MaterialTheme.colorScheme.background
    Box(modifier.size(18.dp).background(colors.container(state).compositeOver(background), CircleShape)
        .border(2.dp, background, CircleShape), contentAlignment = Alignment.Center) {
        Text(number, style = MaterialTheme.typography.labelSmall.copy(fontWeight = FontWeight.Bold, fontFeatureSettings = "tnum"),
            color = colors.color(state))
    }
}

@Composable private fun PathNode(name: String, icon: Int, hue: IconHue, number: String?, section: PathSectionView?,
    click: () -> Unit, ring: Boolean = false, compact: Boolean = false, width: Dp = 72.dp) {
    val colors = LocalStatusColors.current
    val state = section?.state ?: StepState.BLOCKED
    val description = stringResource(stateLabel(state))
    val tileSize = if (compact) 36.dp else 44.dp
    Column(Modifier.width(width).clickable(role = Role.Button, onClick = click).semantics { stateDescription = description }
        .padding(top = 14.dp), horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(4.dp)) {
        Box(Modifier.size(tileSize), contentAlignment = Alignment.Center) {
            // Border centre is 4 dp beyond the tile; both outlines use the same concentric shape.
            if (ring) Box(Modifier.requiredSize(tileSize + 10.5.dp).border(2.5.dp, colors.color(state), RelayTokens.tile))
            IconTile(icon, hue, Modifier.size(tileSize))
            // The badge centre sits 4 dp outside the tile corner, clear of the icon even at 36 dp.
            if (number != null) PathBadge(number, state, Modifier.align(Alignment.TopEnd).offset(x = 13.dp, y = (-13).dp))
        }
        Text(name, style = RelayTokens.caption, maxLines = 1, overflow = TextOverflow.Ellipsis)
    }
}

@Composable private fun PathLink(section: PathSectionView?, number: String, phase: State<Float>, segment: Int,
    compact: Boolean, modifier: Modifier, click: () -> Unit) {
    val status = LocalStatusColors.current
    val steps = section?.steps.orEmpty()
    val failed = steps.indexOfFirst { it.state == StepState.FAILED }
    val current = steps.indexOfFirst { it.state == StepState.ACTION }.takeIf { it >= 0 }
        ?: steps.indexOfFirst { it.state == StepState.WORKING || it.state == StepState.WAITING }
    val track = MaterialTheme.colorScheme.outlineVariant
    val flow = status.ok
    val label = section?.let { stringResource(sectionTitle(it.section)) }.orEmpty()
    val state = stringResource(stateLabel(section?.state ?: StepState.BLOCKED))
    Box(modifier.height(if (compact) 50.dp else 58.dp).clickable(role = Role.Button, onClick = click)
        .semantics(mergeDescendants = true) { contentDescription = label; stateDescription = state }) {
        PathBadge(number, section?.state ?: StepState.BLOCKED, Modifier.align(Alignment.TopCenter).offset(y = 4.dp))
        Canvas(Modifier.fillMaxWidth().height(6.dp).align(Alignment.Center).offset(y = 7.dp)) {
            val y = size.height / 2
            val width = size.width
            val widths = steps.indices.map { if (it == current) 14.dp.toPx() else 6.dp.toPx() }
            val spacing = ((width - widths.sum()) / (steps.size + 1)).coerceAtLeast(0f)
            var cursor = spacing
            val centres = widths.map { dotWidth -> (cursor + dotWidth / 2).also { cursor += dotWidth + spacing } }
            val stroke = 6.dp.toPx()
            if (failed < 0) drawLine(track, Offset(0f, y), Offset(width, y), stroke, StrokeCap.Round)
            else {
                val gap = centres[failed]
                val end = gap - 10.dp.toPx()
                val start = gap + 10.dp.toPx()
                if (end > 0) drawLine(track, Offset(0f, y), Offset(end, y), stroke, StrokeCap.Round)
                if (start < width) drawLine(track, Offset(start, y), Offset(width, y), stroke, StrokeCap.Round)
            }
            steps.forEachIndexed { index, step ->
                val x = centres[index]
                val color = status.color(step.state)
                if (index == current) drawLine(color, Offset(x - 4.dp.toPx(), y), Offset(x + 4.dp.toPx(), y), stroke, StrokeCap.Round)
                else drawCircle(color, 3.dp.toPx(), Offset(x, y))
            }
            val progress = phase.value * 2f - segment
            if (phase.value > 0f && progress in 0f..1f && failed < 0) {
                val x = (progress * (width + 12.dp.toPx()) - 6.dp.toPx()).coerceIn(0f, width)
                drawLine(flow, Offset((x - 3.dp.toPx()).coerceAtLeast(0f), y), Offset((x + 3.dp.toPx()).coerceAtMost(width), y),
                    3.dp.toPx(), StrokeCap.Round)
            }
        }
    }
}
