// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-18

package org.openxplay.direlay.settings

enum class CarPlayResolution(val wire: String) { AUTO("auto"), NATIVE("native"), P1080("1080p"), P720("720p"), SCREEN("screen") }
enum class CarPlayScreenWidth(val mm: Int) { MM125(125), MM200(200), MM250(250), MM300(300) }
enum class ViewAreasMode { AUTOMATIC, MANUAL }

fun physicalWidth(profile: DisplayProfile, screenWidth: CarPlayScreenWidth): Int =
    (kotlin.math.ceil(screenWidth.mm * maxOf(1.0, profile.width / 1920.0) / 25).toInt() * 25).coerceIn(100, 400)

/** DiPlay stores the declared width on a 5 mm grid, so a custom width must already lie on it. */
fun customScreenWidthValid(mm: Int) = mm in 100..400 && mm % 5 == 0

/** The developer calibration wins; a custom width is already in millimetres and is never scaled. */
fun declaredPhysicalWidth(profile: DisplayProfile, screenWidth: CarPlayScreenWidth,
    customMm: Int? = null, developerOverrideMm: Int = 0): Int =
    developerOverrideMm.takeIf { it > 0 } ?: customMm ?: physicalWidth(profile, screenWidth)

/** PictureSize also serves wireless output, whose fitting bounds differ from view-area input. */
fun PictureSize.validateViewArea() { DisplayProfile(width, height, 160).validate() }

fun referenceCarPlayCanvas(running: Boolean, mode: DisplayMode, applied: DisplayProfile?, saved: DisplayProfile): DisplayProfile =
    applied?.takeIf { running && mode == DisplayMode.AUTOMATIC } ?: saved
