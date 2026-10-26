// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-25
package org.openxplay.direlay.settings

import org.junit.Assert.*
import org.junit.Test

class CarlinkPictureSizesTest {
    @Test fun validationMatchesAutomaticFitAndPresetsKeepTheirOrder() {
        assertEquals(listOf(1280 to 720, 1920 to 1080, 1280 to 800, 1920 to 720, 1024 to 600, 1024 to 768),
            CarlinkPictureSizes.presets.map { it.width to it.height })
        for (size in CarlinkPictureSizes.presets + listOf(PictureSize(720, 1280), PictureSize(1080, 1920),
            PictureSize(640, 360), PictureSize(360, 640))) {
            size.validate()
            assertEquals(size, CarlinkPictureSizes.parse(size.width.toString(), size.height.toString()))
        }
        for ((width, height) in listOf(0 to 720, -640 to 360, 1281 to 720, 1280 to 721,
            638 to 360, 640 to 358, 1922 to 1080, 1920 to 1082, 1082 to 1920, 360 to 1922, 360 to 638)) {
            assertNull("$width × $height", CarlinkPictureSizes.parse(width.toString(), height.toString()))
        }
        for (width in listOf("", "abc", "2147483648", "1280.0")) assertNull(CarlinkPictureSizes.parse(width, "720"))
    }
    @Test fun ratiosAreReducedAndUnwieldyLabelsAreOmitted() {
        assertEquals(listOf("16:9", "16:9", "16:10", "8:3", null, "4:3"),
            CarlinkPictureSizes.presets.map { it.ratioLabel })
        assertEquals("9:16", PictureSize(720, 1280).ratioLabel)
        assertNull(PictureSize(1282, 720).ratioLabel)
    }
    @Test fun normalizationDropsInvalidDuplicateAndPresetEntriesBeforeTheLimit() {
        val custom = (0..6).map { PictureSize(1000 + it * 2, 600) }
        val input = listOf(PictureSize(0, 0), CarlinkPictureSizes.presets[0], custom[0], custom[0]) + custom.drop(1)
        assertEquals(custom.take(5), CarlinkPictureSizes.normalized(input))
    }
}
