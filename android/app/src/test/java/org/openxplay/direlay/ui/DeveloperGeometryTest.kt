// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-21
package org.openxplay.direlay.ui

import org.junit.Assert.assertEquals
import org.junit.Test
import org.openxplay.direlay.video.CarlinkCrop
import org.openxplay.direlay.video.CarlinkGeometry
import org.openxplay.stream.core.Viewport

class DeveloperGeometryTest {
    @Test fun liveLineShowsWindowCanvasActiveAreaAndCodedPictureInOrder() {
        val geometry = CarlinkGeometry.calculate(1920, 1080, CarlinkCrop(160, 40, 1440, 960),
            Viewport(1532, 1013), Viewport(1280, 720))
        assertEquals(listOf("1532×1013", "1920×1080", "160,40 1440×960", "1280×720"), carlinkGeometryValues(geometry))
    }
}
