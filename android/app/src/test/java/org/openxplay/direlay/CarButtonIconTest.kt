// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-17
package org.openxplay.direlay

import android.app.Application
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config
import java.io.ByteArrayInputStream
import java.io.File
import javax.imageio.ImageIO

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [33], application = Application::class)
class CarButtonIconTest {
    // DiPlay's CarPlayHostActivity reads its own R.raw.ic_car_home; resource merging resolves that to the
    // relay's file of the same name.
    private val merged get() = RuntimeEnvironment.getApplication().resources
        .openRawResource(com.shilapi.xcertplay.host.R.raw.ic_car_home).use { it.readBytes() }

    @Test fun diplaysPackagedCarButtonIconIsTheRelayIcon() {
        assertArrayEquals(File("src/main/res/raw/ic_car_home.png").readBytes(), merged)
    }

    @Test fun iconIsAnOpaqueSquareDiplaySendsUnscaled() {
        val image = ImageIO.read(ByteArrayInputStream(merged))
        // DiPlay's AirPlayIconSizing sends square PNGs of up to 192 pixels as they are.
        assertEquals(192, image.width)
        assertEquals(192, image.height)
        assertFalse(image.colorModel.hasAlpha())
        // The launcher icon's background colour fills the corners.
        for ((x, y) in listOf(0 to 0, 191 to 0, 0 to 191, 191 to 191)) {
            assertEquals(0x070707, image.getRGB(x, y) and 0xFFFFFF)
        }
    }
}
