// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-11-02
package org.openxplay.direlay.ui

import org.junit.Assert.*
import org.junit.Test

class PreviewBackPlacementTest {
    @Test fun portraitPhoneWithWideCanvasUsesTopBand() {
        assertEquals(PreviewBackPlacement(4f, 163.5f, false), previewBackPlacement(400f, 900f, 1920, 720))
    }

    @Test fun landscapePhoneWithSlightlyWiderCanvasUsesPictureCornerAndScrim() {
        assertEquals(PreviewBackPlacement(4f, 16.5f, true), previewBackPlacement(900f, 400f, 2400, 1000))
    }

    @Test fun narrowerCanvasUsesStartBand() {
        assertEquals(PreviewBackPlacement(41f, 4f, false), previewBackPlacement(900f, 400f, 1280, 800))
        assertEquals(PreviewBackPlacement(811f, 4f, false), previewBackPlacement(900f, 400f, 1280, 800, rtl = true))
    }

    @Test fun bandsMustHaveAtLeast56Dp() {
        assertFalse(previewBackPlacement(400f, 312f, 400, 200).scrim)
        assertTrue(previewBackPlacement(400f, 311f, 400, 200).scrim)
        assertFalse(previewBackPlacement(512f, 400f, 400, 400).scrim)
        assertTrue(previewBackPlacement(511f, 400f, 400, 400).scrim)
    }

    @Test fun barsAndCutoutsReduceUsableBandsWithoutChangingThePictureAspect() {
        assertEquals(PreviewBackPlacement(36f, 175.5f, false),
            previewBackPlacement(400f, 900f, 1920, 720, PreviewInsets(32f, 24f, 16f, 20f)))
        assertEquals(PreviewBackPlacement(51f, 28f, false),
            previewBackPlacement(900f, 400f, 1280, 800, PreviewInsets(20f, 24f, 30f, 16f)))
        assertEquals(PreviewBackPlacement(134f, 28f, true),
            previewBackPlacement(900f, 400f, 1280, 800, PreviewInsets(80f, 24f, 30f, 16f)))
        assertEquals(PreviewBackPlacement(796f, 28f, false),
            previewBackPlacement(900f, 400f, 1280, 800, PreviewInsets(20f, 24f, 30f, 16f), rtl = true))
        assertTrue(previewBackPlacement(400f, 312f, 400, 200, PreviewInsets(top = 1f)).scrim)
    }

    @Test fun fallbackStaysInsideAllInsetsAndHandlesAnUnknownCanvas() {
        assertEquals(PreviewBackPlacement(34f, 28f, true),
            previewBackPlacement(900f, 400f, 2400, 1000, PreviewInsets(30f, 24f, 10f, 16f)))
        assertEquals(PreviewBackPlacement(22f, 8f, true),
            previewBackPlacement(100f, 64f, 640, 640, PreviewInsets(top = 8f, bottom = 8f)))
        assertEquals(PreviewBackPlacement(12f, 28f, true),
            previewBackPlacement(400f, 900f, 0, 0, PreviewInsets(8f, 24f, 8f, 20f)))
        assertEquals(PreviewBackPlacement(336f, 28f, true),
            previewBackPlacement(400f, 900f, 0, 720, PreviewInsets(8f, 24f, 12f, 20f), rtl = true))
    }
}
