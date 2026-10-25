// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-25
package org.openxplay.direlay.relay

import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow
import org.openxplay.direlay.settings.*

/** Owned by one runtime, never included in settings or a configuration file. */
class CarlinkPictureSelection {
    private val selected = MutableStateFlow<CarlinkPictureTrial?>(null)
    val trial = selected.asStateFlow()
    fun choose(value: CarlinkPictureTrial?) { selected.value = value }
    fun stopped() { selected.value = null }
}

fun carlinkPictureScreen(settings: RelaySettings, trial: CarlinkPictureTrial?, primary: Boolean,
    starting: Boolean = false, decoderLimit: Int? = null): CarPlayScreen? {
    if (settings.displayMode == DisplayMode.FIXED || !settings.carlinkOutput ||
        !(primary || starting && !settings.browserOutput)) return null
    val size = settings.effectiveCarlinkPicture(trial) ?: return null
    return CarPlayScreen(CarDisplayFit.fit(size.width, size.height, decoderLimit), "carlink picture")
}

/** Reports and a parked default must not replace the running picture; physical width still applies. */
fun carlinkPictureRefitTarget(applied: DisplayTarget?, automatic: DisplayTarget?, width: CarPlayScreenWidth,
    inEffect: Boolean): DisplayTarget? = if (inEffect && applied != null) applied.copy(screenWidth = width) else automatic
