// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-15
package org.openxplay.direlay.ui

import androidx.compose.animation.core.*
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import org.openxplay.direlay.R
import org.openxplay.direlay.settings.WirelessNetworkChoice

fun networkDescription(choice: WirelessNetworkChoice): Int = when (choice) {
    WirelessNetworkChoice.WIFI_DIRECT -> R.string.network_direct_description
    WirelessNetworkChoice.PHONE_HOTSPOT -> R.string.network_hotspot_description
    WirelessNetworkChoice.EXISTING_WIFI -> R.string.network_wifi_description
}

@Composable fun NetworkDiagram(choice: WirelessNetworkChoice, active: Boolean) {
    val description = stringResource(networkDescription(choice))
    val color = if (active) MaterialTheme.colorScheme.primary else MaterialTheme.colorScheme.onSurfaceVariant
    val scale = LocalDensity.current.fontScale.coerceAtLeast(1f)
    val hub = choice == WirelessNetworkChoice.EXISTING_WIFI
    val phase = if (active && LocalRelayMotion.current) {
        val transition = rememberInfiniteTransition(label = "network links")
        val progress by transition.animateFloat(0f, 1f,
            infiniteRepeatable(tween(1600, easing = LinearEasing)), label = "network travel")
        progress
    } else null
    BoxWithConstraints(Modifier.fillMaxWidth().height((if (hub) 174.dp else 140.dp) * scale)
        .clearAndSetSemantics { contentDescription = description }) {
        // Captions yield to icons before text could collide, including at 200% font scale.
        val captions = maxWidth.value / scale >= 240f
        val y = if (hub) 100.dp * scale else 64.dp * scale
        Canvas(Modifier.fillMaxSize()) {
            val centers = listOf(.16f, .5f, .84f).map { Offset(size.width * it, y.toPx()) }
            val pairs = if (hub) centers.map { Offset(size.width / 2, (22.dp * scale).toPx()) to it }
                else if (choice == WirelessNetworkChoice.PHONE_HOTSPOT)
                    listOf(centers[0] to centers[1], centers[2] to centers[1])
                else listOf(centers[0] to centers[1], centers[1] to centers[2])
            pairs.forEach { (from, to) ->
                val vector = to - from
                val unit = vector / vector.getDistance()
                val start = from + unit * 25.dp.toPx()
                val end = to - unit * 25.dp.toPx()
                drawLine(color, start, end, 1.5.dp.toPx(), StrokeCap.Round)
                if (choice == WirelessNetworkChoice.PHONE_HOTSPOT) {
                    val normal = Offset(-unit.y, unit.x)
                    drawLine(color, end, end - unit * 5.dp.toPx() + normal * 3.dp.toPx(), 1.5.dp.toPx(), StrokeCap.Round)
                    drawLine(color, end, end - unit * 5.dp.toPx() - normal * 3.dp.toPx(), 1.5.dp.toPx(), StrokeCap.Round)
                }
                phase?.let { drawCircle(color, 2.5.dp.toPx(), start + (end - start) * it) }
            }
        }
        if (captions && !hub) {
            listOf(.33f, .67f).forEachIndexed { index, fraction ->
                Text(stringResource(if (choice == WirelessNetworkChoice.PHONE_HOTSPOT) R.string.network_hotspot
                    else if (index == 0) R.string.network_direct else R.string.network_wifi),
                    Modifier.absoluteOffset(x = maxWidth * fraction - maxWidth * .16f, y = 0.dp).width(maxWidth * .32f),
                    style = MaterialTheme.typography.labelSmall, textAlign = TextAlign.Center, maxLines = 2, color = color)
            }
        }
        if (hub) {
            // The hub caption sits beside the router so the links leaving it downwards never cross it.
            DiagramNode(R.drawable.symbol_router, R.string.network_wifi, false, false, false,
                Modifier.absoluteOffset(x = maxWidth * .34f, y = 0.dp).width(maxWidth * .32f))
            if (captions) Text(stringResource(R.string.network_wifi),
                Modifier.absoluteOffset(x = maxWidth * .5f + 28.dp, y = 10.dp * scale),
                style = MaterialTheme.typography.labelSmall, maxLines = 1, color = color)
        }
        listOf(.16f, .5f, .84f).forEachIndexed { index, fraction ->
            DiagramNode(if (index == 2) R.drawable.symbol_car else R.drawable.symbol_phone,
                when (index) { 0 -> R.string.network_iphone; 1 -> R.string.network_this_phone; else -> R.string.network_car },
                index == 1, index == 1 && choice == WirelessNetworkChoice.PHONE_HOTSPOT, captions,
                Modifier.absoluteOffset(x = maxWidth * (fraction - .16f), y = y - 20.dp).width(maxWidth * .32f))
        }
    }
}

@Composable private fun DiagramNode(icon: Int, caption: Int, relay: Boolean, hotspot: Boolean,
    captions: Boolean, modifier: Modifier) {
    Column(modifier, horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(4.dp)) {
        Box {
            Surface(Modifier.size(40.dp), shape = RoundedCornerShape(10.dp),
                color = if (relay) MaterialTheme.colorScheme.primaryContainer else MaterialTheme.colorScheme.surface,
                contentColor = if (relay) MaterialTheme.colorScheme.onPrimaryContainer else MaterialTheme.colorScheme.onSurfaceVariant) {
                Box(contentAlignment = Alignment.Center) { Symbol(icon, null, Modifier.size(24.dp)) }
            }
            if (hotspot) Surface(Modifier.align(Alignment.TopEnd).offset(x = 10.dp, y = (-10).dp),
                shape = RoundedCornerShape(8.dp), color = MaterialTheme.colorScheme.surface) {
                Symbol(R.drawable.symbol_hotspot, null, Modifier.size(24.dp))
            }
        }
        if (captions) Text(stringResource(caption), style = MaterialTheme.typography.labelSmall,
            textAlign = TextAlign.Center, maxLines = 2)
    }
}
