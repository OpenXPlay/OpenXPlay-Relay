// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-15
package org.openxplay.direlay.vehicle

import org.openxplay.stream.core.*

/** Reproducible raw wire data for UI review; the runtime supplies the tick, never randomness. */
fun sampleVehicleData(tick: Long): List<VehicleData> {
    val speed = (40 + tick.mod(40)).toUInt()
    val night = tick.mod(20) >= 10
    return listOf(
        VehicleData.Gear(GearPosition.D, speed),
        VehicleData.Gps(12, 31230400, 121473700, 1u, 2u, speed, 90u, 8u, 12u, tick.toULong()),
        VehicleData.Lights(night, false, true, false, tick.mod(8) == 0),
        VehicleData.DayNight(if (night) DayNightMode.NIGHT else DayNightMode.DAY),
        VehicleData.Fuel(100u, 65u),
        VehicleData.Gyroscope(GyroscopeType.THREE_AXIS, 0.1, tick.mod(4) * 0.25, -0.1, tick.toULong()),
        VehicleData.Acceleration(0.0, 0.2, 9.8, tick.toULong()),
        VehicleData.LightSensor(1000.0, 0.0, if (night) 12.0 else 650.0),
    )
}
