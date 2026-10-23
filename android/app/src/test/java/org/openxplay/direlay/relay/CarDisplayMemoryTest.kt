// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-22
package org.openxplay.direlay.relay

import org.junit.Assert.*
import org.junit.Test
import org.openxplay.stream.core.*

class CarDisplayMemoryTest {
    private fun capabilities(width: Int = 1280, height: Int = 720, latency: LatencyClass = LatencyClass.INTERACTIVE) =
        OutputCapabilities(latency, listOf(DecoderSupport("avc1.64002a", true)),
            mapOf(ScreenId.MAIN to DisplayInfo(width, height, 160, 60.0)), 0, emptySet(), emptySet(), false)
    private val input = InputCapabilities(setOf(VideoCodec.H264), setOf(ScreenId.MAIN), false, emptySet(), false, true, ReconfigureCost.NONE)

    @Test fun castAndMiracastNeverReplaceRememberedBrowserMeasurementsEvenWithIdenticalCapabilities() {
        val memory = CarDisplayMemory()
        var remembered = DisplayInfo(1920, 720, 160, 60.0)
        for (id in listOf("cast", "miracast")) {
            val caps = capabilities()
            val inner = RecordingOutput(id, caps)
            val output = memory.exclude(inner)
            assertEquals(id, output.id)
            val forwarded = mutableListOf<OutputCapabilities>()
            output.open(object : OutputHost {
                override fun capabilities(capabilities: OutputCapabilities) {
                    forwarded.add(capabilities)
                    // Capture precedes negotiation, so even an immediate memory refresh cannot save it.
                    memory.measuredDisplay(capabilities)?.let { remembered = it }
                }
                override fun control(event: ControlEvent) {}
                override fun stats(stats: OutputStats) {}
            })
            assertSame(caps, forwarded.single())
            assertEquals(DisplayInfo(1920, 720, 160, 60.0), remembered)
            assertNull(memory.measuredDisplay(caps))
            val browser = caps.copy()
            assertEquals(caps, browser)
            assertSame(browser.displays[ScreenId.MAIN], memory.measuredDisplay(browser))
            // An older report can still be the negotiated primary during a replacement or retirement.
            val replacement = capabilities(1920, 1080)
            checkNotNull(inner.host).capabilities(replacement)
            assertNull(memory.measuredDisplay(caps)); assertNull(memory.measuredDisplay(replacement))
            output.close()
            assertNull(memory.measuredDisplay(caps)); assertNull(memory.measuredDisplay(replacement))
            assertTrue(inner.closed)
        }
        assertNull(memory.measuredDisplay(null))
    }

    @Test fun browserStillOwnsTheCanvasBeforeTheWirelessDisplaysAndBecomesRememberable() {
        val memory = CarDisplayMemory()
        val policy = CarDisplayPolicy(secondary = memory::isWireless)
        val cast = capabilities(latency = LatencyClass.BUFFERED)
        val miracast = capabilities(1920, 1080)
        val browser = capabilities(1920, 720)
        for ((id, caps) in listOf("cast" to cast, "miracast" to miracast)) {
            memory.exclude(RecordingOutput(id, caps)).open(object : OutputHost {
                override fun capabilities(capabilities: OutputCapabilities) {}
                override fun control(event: ControlEvent) {}
                override fun stats(stats: OutputStats) {}
            })
        }
        policy.derive(input, listOf(cast, miracast))
        assertSame(miracast, policy.primaryOutput)
        assertNull(memory.measuredDisplay(policy.primaryOutput))
        val configuration = policy.derive(input, listOf(cast, miracast, browser))
        assertEquals(1920, configuration.screens[ScreenId.MAIN]?.width)
        assertEquals(720, configuration.screens[ScreenId.MAIN]?.height)
        assertSame(browser, policy.primaryOutput)
        assertSame(browser.displays[ScreenId.MAIN], memory.measuredDisplay(policy.primaryOutput))
        assertEquals(1920, policy.carDisplay?.width)
        assertEquals(720, policy.carDisplay?.height)
        policy.derive(input, listOf(cast))
        assertNull(memory.measuredDisplay(policy.primaryOutput))
    }
}
