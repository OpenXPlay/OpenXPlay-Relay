// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-18

package org.openxplay.direlay.settings

enum class CarPlayResolution(val wire: String) { AUTO("auto"), NATIVE("native"), P1080("1080p"), P720("720p"), SCREEN("screen") }
enum class CarPlayIconSize(val wire: String, val factor: Double) { SMALL("small", 1.4), STANDARD("standard", 1.0), LARGE("large", 0.75) }
fun physicalWidth(profile: DisplayProfile, size: CarPlayIconSize): Int =
    kotlin.math.round(profile.width * 25.4 / profile.density * size.factor).toInt()
