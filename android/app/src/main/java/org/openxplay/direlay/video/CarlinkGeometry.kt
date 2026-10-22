// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-21
package org.openxplay.direlay.video

import org.openxplay.direlay.settings.CarDisplayFit
import org.openxplay.stream.core.*
import kotlin.math.sqrt

data class CarlinkCrop(val x: Int, val y: Int, val width: Int, val height: Int)
data class CarlinkGeometry(val canvas: Viewport, val area: CarlinkCrop, val window: Viewport, val picture: Viewport) {
    val identity get() = area == CarlinkCrop(0, 0, canvas.width, canvas.height) && picture == canvas
    fun map(contact: TouchContact) = contact.copy(x = (area.x + contact.x * area.width) / canvas.width,
        y = (area.y + contact.y * area.height) / canvas.height)
    companion object {
        fun calculate(width: Int, height: Int, area: CarlinkCrop, window: Viewport,
            frameSize: Viewport? = null, decoderLimit: Int? = null): CarlinkGeometry {
            require(width > 0 && height > 0 && area.x >= 0 && area.y >= 0 && area.width >= 2 && area.height >= 2 &&
                area.x.toLong() + area.width <= width && area.y.toLong() + area.height <= height)
            require(decoderLimit == null || decoderLimit > 0)
            require(window.width > 0 && window.height > 0)
            var scale = if (decoderLimit == null) 1.0 else minOf(1.0, sqrt(decoderLimit * 256.0 / (area.width.toDouble() * area.height)))
            fun size() = Viewport((area.width * scale / 2).toInt().coerceAtLeast(1) * 2,
                (area.height * scale / 2).toInt().coerceAtLeast(1) * 2)
            var picture = frameSize ?: size()
            if (frameSize == null && decoderLimit != null) while (CarDisplayFit.macroblocks(picture.width, picture.height) > decoderLimit) {
                scale *= .999
                picture = size()
                require(picture.width > 2 || picture.height > 2) { "decoder_frame_limit" }
            }
            require(picture.width > 0 && picture.height > 0)
            return CarlinkGeometry(Viewport(width, height), area, window, picture)
        }
    }
}
