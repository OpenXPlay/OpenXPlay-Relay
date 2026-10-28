// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-22
package org.openxplay.direlay.miracast

import android.app.Application
import android.os.Handler
import android.os.Looper
import android.view.Surface
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import org.openxplay.stream.core.*
import org.robolectric.RobolectricTestRunner
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config
import org.robolectric.annotation.LooperMode
import java.time.Duration

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [33], application = Application::class)
@LooperMode(LooperMode.Mode.PAUSED)
class MiracastDisplayControllerTest {
    private val car = MiracastDisplay(2, "Car", 1920, 1080, 240, 60.0)
    private class Displays : MiracastDisplays {
        var values = emptyList<MiracastDisplay>()
        var descriptions: List<String>? = null
        var changed: () -> Unit = {}
        var closed = false
        override fun displays() = values
        override fun descriptions(): List<String> = descriptions ?: values.map {
            "${it.id}:${it.name}:${it.width}x${it.height}:flags=8:presentation=true"
        }
        override fun start(changed: () -> Unit) { this.changed = changed }
        override fun close() { closed = true; changed = {} }
        fun update(vararg values: MiracastDisplay) { this.values = values.toList(); changed() }
    }
    private class Window(val surface: (Surface?) -> Unit) : MiracastWindow {
        var size = Viewport(0, 0)
        var closed = false
        override fun videoSize(width: Int, height: Int) { size = Viewport(width, height) }
        override fun close() { closed = true }
    }
    private class Fixture(picture: MiracastPicture = MiracastPicture()) {
        val source = Displays()
        val events = MiracastEvents()
        val windows = mutableListOf<Window>()
        val capabilities = mutableListOf<OutputCapabilities>()
        val surfaces = mutableListOf<Surface?>()
        var allowed = true
        var broken = false
        val controller = MiracastDisplayController(Handler(Looper.getMainLooper()), source,
            MiracastWindow.Factory { _, surface ->
                assertSame(Looper.getMainLooper(), Looper.myLooper())
                if (broken) throw IllegalStateException("window unavailable")
                Window(surface).also { windows += it }
            }, { allowed }, listOf(DecoderSupport("avc1.64002a", true)), events,
            { capabilities += it }, { surfaces += it }, picture)
        fun poll() = shadowOf(Looper.getMainLooper()).idleFor(Duration.ofSeconds(2))
    }
    @Test fun displayDiagnosticsReportAllKnownDisplaysOnlyWhenTheirDescriptionsChange() {
        val f = Fixture()
        f.controller.start()
        assertEquals(listOf("displays none"), f.events.diagnostics)
        f.source.changed()
        assertEquals(1, f.events.diagnostics.size)
        val phone = "0:Phone:1080x2400:flags=3:presentation=false"
        val unmarked = "5:Cast:1280x720:flags=0:presentation=false"
        val presentation = "2:Car:1920x1080:flags=8:presentation=true"
        val descriptions = mutableListOf(phone, unmarked)
        f.source.descriptions = descriptions
        f.source.changed()
        assertEquals("displays $phone; $unmarked", f.events.diagnostics.last())
        assertTrue(f.windows.isEmpty())
        descriptions += presentation
        f.source.update(car)
        assertEquals("displays $phone; $unmarked; $presentation", f.events.diagnostics[2])
        assertEquals("window shown on 2 1920x1080", f.events.diagnostics[3])
        f.poll(); f.poll()
        f.source.descriptions = descriptions.toList()
        f.source.changed()
        assertEquals(4, f.events.diagnostics.size)
        f.source.descriptions = descriptions
        f.source.changed()
        val resized = unmarked.replace("1280x720", "1920x1080")
        descriptions[1] = resized
        f.source.changed()
        assertEquals("displays $phone; $resized; $presentation", f.events.diagnostics.last())
        assertEquals(1, f.windows.size)
        f.source.descriptions = emptyList()
        f.source.update()
        assertEquals("displays none", f.events.diagnostics.last())
        assertEquals(5, f.events.diagnostics.count { it.startsWith("displays ") })
        f.controller.close()
    }
    @Test fun customPictureUsesDisplayDensityAndRefreshOnlyWhileItsWindowIsShowing() {
        val f = Fixture(MiracastPicture(Viewport(1920, 720), stretch = true))
        f.controller.start()
        assertTrue(f.capabilities.last().displays.isEmpty())
        f.source.update(car)
        assertEquals(car, f.events.states.last().second)
        assertEquals(DisplayInfo(1920, 720, 240, 60.0), f.capabilities.last().displays[ScreenId.MAIN])
        f.source.update(car.copy(width = 1280, height = 720, densityDpi = 160, refreshHz = 50.0))
        assertEquals(DisplayInfo(1920, 720, 160, 50.0), f.capabilities.last().displays[ScreenId.MAIN])
        f.allowed = false; f.poll()
        assertTrue(f.capabilities.last().displays.isEmpty())
        f.allowed = true; f.poll()
        assertEquals(1920, f.capabilities.last().displays[ScreenId.MAIN]?.widthPx)
        f.source.update()
        assertTrue(f.capabilities.last().displays.isEmpty())
        f.controller.close()
    }

    @Test fun presentationAddRemoveAndReplacementUpdateCapabilitiesAndReleaseWindows() {
        val f = Fixture()
        f.controller.start()
        assertEquals(MiracastState.WAITING_FOR_DISPLAY, f.events.states.last().first)
        assertTrue(f.capabilities.last().displays.isEmpty())
        f.source.update(car.copy(id = 0), car, car.copy(id = 3))
        assertEquals(MiracastState.SHOWING to car, f.events.states.last())
        assertEquals(1920, f.capabilities.last().displays[ScreenId.MAIN]!!.widthPx)
        val first = f.windows.single()
        f.source.update(car.copy(id = 3))
        assertTrue(first.closed)
        assertEquals(3, f.events.states.last().second!!.id)
        f.source.update()
        assertTrue(f.windows.last().closed)
        assertTrue(f.capabilities.last().displays.isEmpty())
        assertEquals(listOf("display_lost", "display_lost"), f.events.failures)
        f.controller.close()
        f.controller.close()
        assertTrue(f.source.closed)
        assertEquals(1, f.events.states.count { it.first == MiracastState.ENDED })
        f.poll()
        assertEquals(MiracastState.ENDED, f.events.states.last().first)
    }
    @Test fun permissionIsPolledAndDenialIsReportedOncePerEpisode() {
        val f = Fixture()
        f.allowed = false
        f.controller.start()
        assertEquals(MiracastState.NEEDS_PERMISSION to null, f.events.states.last())
        f.source.update(car)
        assertEquals(MiracastState.NEEDS_PERMISSION to car, f.events.states.last())
        assertTrue(f.capabilities.last().displays.isEmpty())
        f.poll(); f.poll()
        assertEquals(listOf("overlay_denied"), f.events.failures)
        f.allowed = true
        f.poll()
        assertEquals(MiracastState.SHOWING, f.events.states.last().first)
        f.allowed = false
        f.poll()
        assertTrue(f.windows.single().closed)
        assertTrue(f.capabilities.last().displays.isEmpty())
        assertEquals(listOf("overlay_denied", "overlay_denied"), f.events.failures)
        f.controller.close()
    }
    @Test fun failedWindowsRetryAndModeChangesReplaceTheWindow() {
        val f = Fixture()
        f.broken = true
        f.source.update(car)
        f.controller.start()
        val diagnostic = "window failed on 2: IllegalStateException: window unavailable"
        assertEquals(diagnostic, f.events.diagnostics.last())
        f.poll()
        assertEquals(listOf("window_failed"), f.events.failures)
        assertEquals(1, f.events.diagnostics.count { it == diagnostic })
        assertTrue(f.capabilities.last().displays.isEmpty())
        f.broken = false
        f.poll()
        assertEquals(MiracastState.SHOWING, f.events.states.last().first)
        assertEquals("window shown on 2 1920x1080", f.events.diagnostics.last())
        val first = f.windows.single()
        f.source.update(car.copy(width = 1280, height = 720, refreshHz = 50.0))
        assertEquals("window shown on 2 1280x720", f.events.diagnostics.last())
        assertTrue(first.closed)
        assertEquals(DisplayInfo(1280, 720, 240, 50.0), f.capabilities.last().displays[ScreenId.MAIN])
        val count = f.surfaces.size
        first.surface(null)
        assertEquals(count, f.surfaces.size)
        f.controller.close()
    }
    @Test fun endingVideoBlanksTheWindowAndKeepsTheDisplayClaimed() {
        val f = Fixture()
        f.controller.videoSize(640, 480)
        f.source.update(car)
        f.controller.start()
        val window = f.windows.single()
        assertEquals(Viewport(640, 480), window.size)
        f.controller.videoSize(0, 0)
        assertEquals(Viewport(0, 0), window.size)
        assertFalse(window.closed)
        assertEquals(MiracastState.SHOWING, f.events.states.last().first)
        f.controller.videoSize(1280, 720)
        assertSame(window, f.windows.single())
        f.controller.close()
    }
}
