// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-25
package org.openxplay.direlay.ui

import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import kotlinx.coroutines.delay
import org.openxplay.direlay.vehicle.*

@Composable internal fun VehiclePage(view: VehicleView, snapshot: VehicleSnapshot?, snapshotPage: Boolean,
    back: () -> Unit, openSnapshot: (Long) -> Unit, enable: () -> Unit, clear: () -> Unit,
    modifier: Modifier, contentModifier: Modifier) {
    val now by produceState(System.currentTimeMillis()) {
        while (true) { value = System.currentTimeMillis(); delay(1000) }
    }
    VehicleScreen(view, now, snapshot, snapshotPage, back, openSnapshot, enable, clear, modifier, contentModifier)
}
