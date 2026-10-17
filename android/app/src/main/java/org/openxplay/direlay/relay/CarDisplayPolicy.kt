// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-15
package org.openxplay.direlay.relay

import org.openxplay.direlay.settings.CarDisplayFit
import org.openxplay.direlay.settings.DisplayProfile
import org.openxplay.stream.core.*

/** Observes Stream's negotiation without doing preference I/O on hub callbacks. */
class CarDisplayPolicy : SessionPolicy {
    @Volatile var carDisplay: DisplayProfile? = null
        private set

    @Volatile var lastCarDisplay: DisplayProfile? = null
        private set

    @Volatile var activeOutputs: List<OutputCapabilities> = emptyList()
        private set

    override fun derive(input: InputCapabilities, outputs: List<OutputCapabilities>): SessionConfig {
        activeOutputs = outputs.filter { ScreenId.MAIN in it.displays }
        val config = DefaultSessionPolicy.derive(input, outputs)
        // A policy fallback is not evidence of a connected car. The local preview is a direct
        // DiPlay sink, never a hub destination; the status observer reports no displays either.
        // Every receiver of the main screen must decode the canvas: use the smallest H.264 frame-size
        // limit among them (each output counts its most capable H.264 decoder).
        val limit = outputs.filter { ScreenId.MAIN in it.displays }
            .mapNotNull { output -> output.decoders.mapNotNull { CarDisplayFit.h264MaxMacroblocks(it.codecString) }.maxOrNull() }
            .minOrNull()
        carDisplay = config.screens[ScreenId.MAIN]?.takeIf {
            outputs.any { ScreenId.MAIN in it.displays }
        }?.let { runCatching { CarDisplayFit.fit(it.width, it.height, limit) }.getOrNull() }
        carDisplay?.let { lastCarDisplay = it }
        return config
    }
}
