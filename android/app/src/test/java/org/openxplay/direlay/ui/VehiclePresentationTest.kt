// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-15
package org.openxplay.direlay.ui

import org.junit.Assert.*
import org.junit.Test
import org.openxplay.direlay.vehicle.*
import org.openxplay.stream.core.*

class VehiclePresentationTest {
    private val reports = listOf(vehicleReport(VehicleData.Gear(GearPosition.D, 42u), 100))
    @Test fun liveScreenHasOneOffOrWaitingStateAndDataOnlyWhileEnabled() {
        assertEquals(VehicleScreenState.OFF, vehicleScreenState(VehicleView()))
        assertEquals(VehicleScreenState.OFF, vehicleScreenState(VehicleView(reports = reports)))
        assertEquals(VehicleScreenState.WAITING, vehicleScreenState(VehicleView(enabled = true)))
        assertEquals(VehicleScreenState.DATA, vehicleScreenState(VehicleView(true, reports)))
        assertEquals(VehicleScreenState.DATA, vehicleScreenState(VehicleView(true, reports, sample = true)))
    }
    @Test fun snapshotsIgnoreLiveCollectionStateAndMissingSnapshotsHaveTheirOwnState() {
        val snapshot = VehicleSnapshot(100, reports)
        assertEquals(VehicleScreenState.DATA, vehicleScreenState(VehicleView(), true, snapshot))
        assertEquals(VehicleScreenState.MISSING, vehicleScreenState(VehicleView(true, reports), true))
        assertEquals(VehicleScreenState.MISSING, vehicleScreenState(VehicleView(), true, snapshot.copy(reports = emptyList())))
    }
}
