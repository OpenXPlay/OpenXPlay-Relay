// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-22
package org.openxplay.direlay.logs

import com.google.zxing.qrcode.decoder.Decoder
import org.junit.Assert.*
import org.junit.Test

class LogQrCodeTest {
    @Test fun matrixHasTwoModuleQuietZoneAndDecodesTheOriginalUrl() {
        val url = "https://d1.tfdl.net/test%20archive.zip"
        val matrix = LogQrCode.matrix(url)
        assertEquals(matrix.width, matrix.height)
        for (position in 0 until matrix.width) for (edge in 0..1) {
            assertFalse(matrix[position, edge]); assertFalse(matrix[edge, position])
            assertFalse(matrix[position, matrix.height - 1 - edge]); assertFalse(matrix[matrix.width - 1 - edge, position])
        }
        val payload = com.google.zxing.common.BitMatrix(matrix.width - 4)
        for (y in 0 until payload.height) for (x in 0 until payload.width) if (matrix[x + 2, y + 2]) payload.set(x, y)
        assertEquals(url, Decoder().decode(payload).text)
    }
}
