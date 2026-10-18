// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-15
package org.openxplay.direlay.relay

import org.openxplay.direlay.settings.CarPlayResolution
import org.openxplay.direlay.settings.CarDisplayFit
import org.openxplay.direlay.settings.DisplayProfile
import org.openxplay.stream.core.*

/** Observes Stream's negotiation without doing preference I/O on hub callbacks. */
class CarDisplayPolicy(private val observe: (DisplayInfo?) -> Unit = {}, private val wholeScreen: () -> Boolean = { false }, private val resolution: () -> CarPlayResolution = { CarPlayResolution.AUTO }) : SessionPolicy {
    @Volatile var carDisplay: DisplayProfile? = null
        private set

    @Volatile var lastCarDisplay: DisplayProfile? = null
        private set

    @Volatile var activeOutputs: List<OutputCapabilities> = emptyList()
        private set

    @Synchronized override fun derive(input: InputCapabilities, outputs: List<OutputCapabilities>): SessionConfig {
        activeOutputs = outputs.filter { ScreenId.MAIN in it.displays }
        val config = DefaultSessionPolicy.derive(input, outputs)
        refreshDisplay()
        observe(primaryDisplay)
        return config
    }
    val primaryDisplay: DisplayInfo? get() = activeOutputs.firstOrNull { it.latency == LatencyClass.INTERACTIVE }?.displays?.get(ScreenId.MAIN)
        ?: activeOutputs.firstOrNull()?.displays?.get(ScreenId.MAIN)

    @Synchronized fun refreshDisplay() {
        // Every receiver of the main screen must decode the canvas: use the smallest H.264 frame-size
        // limit among them (each output counts its most capable H.264 decoder).
        val outputs = activeOutputs.filter { ScreenId.MAIN in it.displays }
        val limit = outputs.mapNotNull { output ->
            output.decoders.mapNotNull { CarDisplayFit.h264MaxMacroblocks(it.codecString) }.maxOrNull()
        }.minOrNull()
        carDisplay = primaryDisplay?.let { fitDisplay(it, resolution(), limit, wholeScreen()) }
        carDisplay?.let { lastCarDisplay = it }
    }
}

fun fitDisplay(display: DisplayInfo, resolution: CarPlayResolution, limit: Int? = null, wholeScreen: Boolean = false): DisplayProfile? {
    val viewport = display.viewport.takeUnless { wholeScreen || resolution == CarPlayResolution.SCREEN }
    val width = viewport?.width ?: display.widthPx
    val height = viewport?.height ?: display.heightPx
    val cap = when (resolution) { CarPlayResolution.P1080 -> 1080; CarPlayResolution.P720 -> 720; else -> Int.MAX_VALUE }
    val scale = minOf(1.0, cap.toDouble() / minOf(width, height))
    return runCatching { CarDisplayFit.fit((width * scale).toInt().coerceAtLeast(1),
        (height * scale).toInt().coerceAtLeast(1), limit) }.getOrNull()
}
