// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-24
package org.openxplay.direlay.ui.design

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.*
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
            PathLink(path.section(PathSection.IPHONE), "③", phase, 0, compact, Modifier.weight(1f)) { onSection(PathSection.IPHONE) }
            PathNode(path.nodes.relay ?: stringResource(R.string.design_section_relay), R.drawable.symbol_phone,
                IconHue.NETWORK, "①", path.section(PathSection.RELAY), { onSection(PathSection.RELAY) }, ring = true, compact = compact, width = nodeWidth)
            PathLink(path.section(PathSection.CAR), "②", phase, 1, compact, Modifier.weight(1f)) { onSection(PathSection.CAR) }
            PathNode(path.nodes.car ?: stringResource(R.string.design_section_car), R.drawable.symbol_car_front,
                IconHue.CAR, "④", path.section(PathSection.PICTURE), { onSection(PathSection.PICTURE) }, compact = compact, width = nodeWidth)
        }
    }
}

@Composable private fun PathNode(name: String, icon: Int, hue: IconHue, number: String?, section: PathSectionView?,
    click: () -> Unit, ring: Boolean = false, compact: Boolean = false, width: Dp = 72.dp) {
    val colors = LocalStatusColors.current
    val dots = section?.steps?.map { colors.color(it.state) }.orEmpty()
    val state = section?.let { stringResource(stateLabel(it.state)) }.orEmpty()
    Column(Modifier.width(width).clickable(role = Role.Button, onClick = click).semantics { stateDescription = state }
        .heightIn(min = 48.dp).padding(vertical = if (compact) 0.dp else 4.dp), horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(4.dp)) {
        Box(Modifier.size(if (compact) 44.dp else 48.dp), contentAlignment = Alignment.Center) {
            IconTile(icon, hue, Modifier.size(44.dp))
            if (ring) Canvas(Modifier.matchParentSize()) {
                val count = dots.size.coerceAtLeast(1)
                dots.forEachIndexed { i, color -> drawArc(color, -90f + 360f * i / count, 360f / count - 5f,
                    false, topLeft = Offset(1.dp.toPx(), 1.dp.toPx()), size = Size(size.width - 2.dp.toPx(), size.height - 2.dp.toPx()),
                    style = Stroke(2.dp.toPx(), cap = StrokeCap.Round)) }
            }
            if (number != null) Text(number, Modifier.align(Alignment.TopEnd), style = RelayTokens.caption,
                color = MaterialTheme.colorScheme.onSurface)
        }
        Text(name, style = RelayTokens.caption, maxLines = 1, overflow = TextOverflow.Ellipsis)
    }
}

@Composable private fun PathLink(section: PathSectionView?, number: String, phase: State<Float>, segment: Int, compact: Boolean, modifier: Modifier, click: () -> Unit) {
    val status = LocalStatusColors.current
    val dots = section?.steps?.map { status.color(it.state) }.orEmpty()
    val failed = section?.steps?.indexOfFirst { it.state == StepState.FAILED } ?: -1
    val track = MaterialTheme.colorScheme.outlineVariant
    val flow = status.ok
    val label = section?.let { stringResource(sectionTitle(it.section)) }.orEmpty()
    val state = section?.let { stringResource(stateLabel(it.state)) }.orEmpty()
    Box(modifier.height(if (compact) 48.dp else 56.dp).clickable(role = Role.Button, onClick = click)
        .semantics(mergeDescendants = true) { contentDescription = label; stateDescription = state }) {
        Text(number, Modifier.align(Alignment.TopCenter).offset(y = if (compact) (-4).dp else 0.dp), style = RelayTokens.caption, color = MaterialTheme.colorScheme.onSurfaceVariant)
        Canvas(Modifier.fillMaxWidth().height(8.dp).align(Alignment.Center).offset(y = if (compact) (-2).dp else 0.dp)) {
            val y = size.height / 2
            val width = size.width
            val count = dots.size.coerceAtLeast(1)
            if (failed < 0) drawLine(track, Offset(0f, y), Offset(width, y), 8.dp.toPx(), StrokeCap.Round)
            else {
                val gap = width * (failed + .5f) / count
                drawLine(track, Offset(0f, y), Offset((gap - 5.dp.toPx()).coerceAtLeast(0f), y), 8.dp.toPx(), StrokeCap.Round)
                drawLine(track, Offset((gap + 5.dp.toPx()).coerceAtMost(width), y), Offset(width, y), 8.dp.toPx(), StrokeCap.Round)
            }
            dots.forEachIndexed { index, color -> drawCircle(color, 2.dp.toPx(), Offset(width * (index + .5f) / count, y)) }
            val progress = phase.value * 2f - segment
            if (phase.value > 0f && progress in 0f..1f && failed < 0) {
                val x = (progress * (width + 12.dp.toPx()) - 6.dp.toPx()).coerceIn(0f, width)
                drawLine(flow, Offset((x - 3.dp.toPx()).coerceAtLeast(0f), y), Offset((x + 3.dp.toPx()).coerceAtMost(width), y), 3.dp.toPx(), StrokeCap.Round)
            }
        }
    }
}
