// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-22
package org.openxplay.direlay.miracast

import android.app.Application
import android.hardware.display.DisplayManager
import android.os.Handler
import android.os.Looper
import android.util.DisplayMetrics
import android.view.Display
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config
import org.robolectric.annotation.LooperMode
import org.robolectric.shadows.ShadowDisplayManager
import org.robolectric.util.ReflectionHelpers

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [28, 33], application = Application::class)
@LooperMode(LooperMode.Mode.PAUSED)
class AndroidMiracastDisplaysTest {
    @Test fun platformSourceWatchesOnlyPresentationDisplaysAndUsesModePixels() {
        val context = RuntimeEnvironment.getApplication()
        val manager = context.getSystemService(DisplayManager::class.java)
        val source = AndroidMiracastDisplays(context, Handler(Looper.getMainLooper()))
        var changes = 0
        source.start { assertSame(Looper.getMainLooper(), Looper.myLooper()); changes++ }
        try {
            assertTrue(source.displays().isEmpty())
            val ordinary = ShadowDisplayManager.addDisplay("w800dp-h600dp-land-mdpi")
            // The integer argument is a display type, not flags; presentation needs both.
            val wifiType = ReflectionHelpers.getStaticField<Int>(Display::class.java, "TYPE_WIFI")
            val external = ShadowDisplayManager.addDisplay("w1920dp-h1080dp-land-mdpi", wifiType)
            val display = manager.getDisplay(external)
            assertTrue(manager.getDisplays(DisplayManager.DISPLAY_CATEGORY_PRESENTATION).isEmpty())
            assertEquals(1920, display.mode.physicalWidth)
            assertEquals(1080, display.mode.physicalHeight)
            assertEquals(60f, display.mode.refreshRate, 0f)
            assertEquals(160, DisplayMetrics().also { display.getRealMetrics(it) }.densityDpi)
            val initial = source.descriptions()
            assertEquals(manager.displays.size, initial.size)
            assertTrue(initial.any { it.startsWith("0:") && it.endsWith("presentation=false") })
            assertTrue(initial.any { it.startsWith("$ordinary:") && it.endsWith("presentation=false") })
            assertTrue(initial.any { it.startsWith("$external:") &&
                it.endsWith(":1920x1080:flags=${Integer.toHexString(display.flags)}:presentation=false") })
            shadowOf(display).setFlags(Display.FLAG_PRESENTATION)
            shadowOf(Looper.getMainLooper()).idle()
            assertEquals(listOf(external), manager.getDisplays(DisplayManager.DISPLAY_CATEGORY_PRESENTATION).map { it.displayId })
            val selected = requireNotNull(MiracastPolicy.display(source.displays()))
            assertEquals(external, selected.id)
            assertEquals(1920, selected.width)
            assertEquals(1080, selected.height)
            assertEquals(160, selected.densityDpi)
            assertTrue(selected.refreshHz > 0)
            assertTrue(source.displays().none { it.id == Display.DEFAULT_DISPLAY || it.id == ordinary })
            assertTrue(source.descriptions().any { it.startsWith("$external:") && it.endsWith(":1920x1080:flags=8:presentation=true") })
            val before = changes
            ShadowDisplayManager.removeDisplay(external)
            shadowOf(Looper.getMainLooper()).idle()
            assertTrue(changes > before)
            assertNull(MiracastPolicy.display(source.displays()))
            assertTrue(source.descriptions().none { it.startsWith("$external:") })
            source.close()
            val stopped = changes
            ShadowDisplayManager.removeDisplay(ordinary)
            shadowOf(Looper.getMainLooper()).idle()
            assertEquals(stopped, changes)
        } finally { source.close() }
    }
}
