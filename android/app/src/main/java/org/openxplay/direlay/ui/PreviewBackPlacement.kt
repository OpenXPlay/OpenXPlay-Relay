// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-11-02
package org.openxplay.direlay.ui

internal data class PreviewInsets(val left: Float = 0f, val top: Float = 0f,
    val right: Float = 0f, val bottom: Float = 0f)
internal data class PreviewBackPlacement(val left: Float, val top: Float, val scrim: Boolean)

/** Fit CarPlay to the whole view; system insets constrain the button, not the picture's aspect. */
internal fun previewBackPlacement(width: Float, height: Float, canvasWidth: Int, canvasHeight: Int,
    insets: PreviewInsets = PreviewInsets(), rtl: Boolean = false): PreviewBackPlacement {
    val scale = if (canvasWidth > 0 && canvasHeight > 0) minOf(width / canvasWidth, height / canvasHeight) else 0f
    val pictureWidth = if (scale > 0) canvasWidth * scale else width
    val pictureHeight = if (scale > 0) canvasHeight * scale else height
    val pictureLeft = (width - pictureWidth) / 2
    val pictureTop = (height - pictureHeight) / 2
    val safeLeft = insets.left
    val safeRight = width - insets.right
    val safeTop = insets.top
    val safeBottom = height - insets.bottom
    val topBand = pictureTop - safeTop
    val startBand = pictureLeft - if (rtl) insets.right else insets.left
    var left = if (rtl) minOf(safeRight, width - pictureLeft) - 52 else maxOf(safeLeft, pictureLeft) + 4
    var top = maxOf(safeTop, pictureTop) + 4
    val scrim = when {
        topBand >= 56 -> {
            left = if (rtl) safeRight - 52 else safeLeft + 4
            top = safeTop + (topBand - 48) / 2
            false
        }
        startBand >= 56 -> {
            left = if (rtl) safeRight - (startBand + 48) / 2 else safeLeft + (startBand - 48) / 2
            top = safeTop + 4
            false
        }
        else -> true
    }
    return PreviewBackPlacement(left.coerceIn(safeLeft, maxOf(safeLeft, safeRight - 48)),
        top.coerceIn(safeTop, maxOf(safeTop, safeBottom - 48)), scrim)
}
