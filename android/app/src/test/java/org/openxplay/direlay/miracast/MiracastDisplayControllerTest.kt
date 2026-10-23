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
        var changed: () -> Unit = {}
        var closed = false
        override fun displays() = values
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
    private class Fixture {
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
            { capabilities += it }, { surfaces += it })
        fun poll() = shadowOf(Looper.getMainLooper()).idleFor(Duration.ofSeconds(2))
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
        f.poll()
        assertEquals(listOf("window_failed"), f.events.failures)
        assertTrue(f.capabilities.last().displays.isEmpty())
        f.broken = false
        f.poll()
        assertEquals(MiracastState.SHOWING, f.events.states.last().first)
        val first = f.windows.single()
        f.source.update(car.copy(width = 1280, height = 720, refreshHz = 50.0))
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
