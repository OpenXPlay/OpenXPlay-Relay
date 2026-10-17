// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-15
package org.openxplay.direlay.ui

import org.openxplay.direlay.vehicle.VehicleView
import org.openxplay.direlay.vehicle.VehicleSnapshot

enum class VehicleScreenState { OFF, WAITING, DATA, MISSING }
fun vehicleScreenState(view: VehicleView, isSnapshot: Boolean = false, snapshot: VehicleSnapshot? = null) = when {
    isSnapshot -> if (snapshot == null || snapshot.reports.isEmpty()) VehicleScreenState.MISSING else VehicleScreenState.DATA
    !view.enabled -> VehicleScreenState.OFF
    view.reports.isEmpty() -> VehicleScreenState.WAITING
    else -> VehicleScreenState.DATA
}

