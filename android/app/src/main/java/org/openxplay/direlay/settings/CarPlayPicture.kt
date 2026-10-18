// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-18

package org.openxplay.direlay.settings

enum class CarPlayResolution(val wire: String) { AUTO("auto"), NATIVE("native"), P1080("1080p"), P720("720p"), SCREEN("screen") }
enum class CarPlayScreenWidth(val mm: Int) { MM125(125), MM200(200), MM250(250), MM300(300) }
fun physicalWidth(profile: DisplayProfile, screenWidth: CarPlayScreenWidth): Int =
    (kotlin.math.ceil(screenWidth.mm * maxOf(1.0, profile.width / 1920.0) / 25).toInt() * 25).coerceIn(100, 400)
