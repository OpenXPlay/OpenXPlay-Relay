// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-18

package org.openxplay.direlay.settings

enum class CarPlayResolution(val wire: String) { AUTO("auto"), NATIVE("native"), P1080("1080p"), P720("720p"), SCREEN("screen") }
enum class CarPlayIconSize(val wire: String) { SMALL("small"), STANDARD("standard"), LARGE("large") }
fun physicalWidth(profile: DisplayProfile): Int =
    kotlin.math.round(profile.width * 25.4 / profile.density).toInt()
