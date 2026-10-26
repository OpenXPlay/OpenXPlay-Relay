// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-15
package org.openxplay.direlay.ui

import androidx.compose.animation.core.*
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.layout.*
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
import org.openxplay.direlay.ui.design.*

fun networkDescription(choice: WirelessNetworkChoice): Int = when (choice) {
    WirelessNetworkChoice.WIFI_DIRECT -> R.string.network_direct_description
    WirelessNetworkChoice.PHONE_HOTSPOT -> R.string.network_hotspot_description
    WirelessNetworkChoice.EXISTING_WIFI -> R.string.network_wifi_description
}

@Composable fun NetworkDiagram(choice: WirelessNetworkChoice, active: Boolean) {
    val description = stringResource(networkDescription(choice))
    val color = if (active) iconHue(IconHue.NETWORK) else MaterialTheme.colorScheme.onSurfaceVariant
    val track = MaterialTheme.colorScheme.outlineVariant
    val scale = LocalDensity.current.fontScale.coerceAtLeast(1f)
    // On the car's hotspot the car is the hub above the iPhone and this phone.
    val hub = choice == WirelessNetworkChoice.EXISTING_WIFI
    val motion = LocalRelayMotion.current
    val phase = rememberRelayPhase(active, choice, periodMillis = 1600, durationMillis = 3200)
    val fractions = if (hub) listOf(.3f, .7f) else listOf(.16f, .5f, .84f)
    BoxWithConstraints(Modifier.fillMaxWidth().height((if (hub) 184.dp else 140.dp) * scale)
        .clearAndSetSemantics { contentDescription = description }) {
        // Captions yield to icons before text could collide, including at 200% font scale.
        val captions = maxWidth.value / scale >= 240f
        val y = if (hub) 110.dp * scale else 64.dp * scale
        val hubTop = 12.dp * scale
        Canvas(Modifier.fillMaxSize()) {
            val centers = fractions.map { Offset(size.width * it, y.toPx()) }
            val pairs = if (hub) centers.map { it to Offset(size.width / 2, (hubTop + 20.dp).toPx()) }
                else if (choice == WirelessNetworkChoice.PHONE_HOTSPOT)
                    listOf(centers[0] to centers[1], centers[2] to centers[1])
                else listOf(centers[0] to centers[1], centers[1] to centers[2])
            pairs.forEach { (from, to) ->
                val vector = to - from
                val unit = vector / vector.getDistance()
                val start = from + unit * 25.dp.toPx()
                val end = to - unit * 25.dp.toPx()
                drawLine(track, start, end, 4.dp.toPx(), StrokeCap.Round)
                drawLine(color, start, end, 2.dp.toPx(), StrokeCap.Round)
                if (hub || choice == WirelessNetworkChoice.PHONE_HOTSPOT) {
                    val normal = Offset(-unit.y, unit.x)
                    drawLine(color, end, end - unit * 5.dp.toPx() + normal * 3.dp.toPx(), 1.5.dp.toPx(), StrokeCap.Round)
                    drawLine(color, end, end - unit * 5.dp.toPx() - normal * 3.dp.toPx(), 1.5.dp.toPx(), StrokeCap.Round)
                }
                if (active && motion) drawCircle(color, 2.5.dp.toPx(), start + (end - start) * phase.value)
            }
        }
        if (captions && !hub) {
            listOf(.33f, .67f).forEachIndexed { index, fraction ->
                Text(stringResource(if (choice == WirelessNetworkChoice.PHONE_HOTSPOT) R.string.network_hotspot
                    else if (index == 0) R.string.network_direct else R.string.network_wifi),
                    Modifier.absoluteOffset(x = maxWidth * fraction - maxWidth * .16f, y = 0.dp).width(maxWidth * .32f),
                    style = RelayTokens.caption, textAlign = TextAlign.Center, maxLines = 2, color = color)
            }
        }
        if (hub) {
            // The car's caption sits beside it, clear of its hotspot badge, so the links arriving from below never cross it.
            DiagramNode(R.drawable.symbol_car_front, R.string.network_car, IconHue.CAR, true, false,
                Modifier.absoluteOffset(x = maxWidth * .34f, y = hubTop).width(maxWidth * .32f))
            if (captions) Text(stringResource(R.string.network_car),
                Modifier.absoluteOffset(x = maxWidth * .5f + 36.dp, y = hubTop + 10.dp),
                style = RelayTokens.caption, maxLines = 1)
        }
        fractions.forEachIndexed { index, fraction ->
            DiagramNode(if (index == 2) R.drawable.symbol_car_front else R.drawable.symbol_smartphone,
                when (index) { 0 -> R.string.network_iphone; 1 -> R.string.network_this_phone; else -> R.string.network_car },
                when (index) { 0 -> IconHue.IPHONE; 1 -> IconHue.NETWORK; else -> IconHue.CAR },
                index == 1 && choice == WirelessNetworkChoice.PHONE_HOTSPOT, captions,
                Modifier.absoluteOffset(x = maxWidth * (fraction - .16f), y = y - 20.dp).width(maxWidth * .32f))
        }
    }
}

@Composable private fun DiagramNode(icon: Int, caption: Int, hue: IconHue, hotspot: Boolean,
    captions: Boolean, modifier: Modifier) {
    Column(modifier, horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(4.dp)) {
        Box {
            IconTile(icon, hue, Modifier.size(40.dp))
            if (hotspot) Surface(Modifier.align(Alignment.TopEnd).offset(x = 10.dp, y = (-10).dp),
                shape = RelayTokens.chip, color = MaterialTheme.colorScheme.primaryContainer,
                contentColor = MaterialTheme.colorScheme.onPrimaryContainer) {
                Symbol(R.drawable.symbol_hotspot, null, Modifier.size(24.dp))
            }
        }
        if (captions) Text(stringResource(caption), style = RelayTokens.caption,
            textAlign = TextAlign.Center, maxLines = 2)
    }
}
