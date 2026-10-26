// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-24
package org.openxplay.direlay.relay

import org.junit.Assert.*
import org.junit.Test
import org.openxplay.direlay.settings.*
import org.openxplay.stream.core.*

class CarFirstPolicyTest {
    @Test fun allStartConditionsMustRequireANewCarScreen() {
        for (automatic in listOf(false, true)) for (browser in listOf(false, true))
            for (carlink in listOf(false, true)) for (reported in listOf(false, true)) for (saved in listOf(false, true)) {
                val policy = CarFirstPolicy()
                val wait = automatic && (browser || carlink) && !reported && !saved
                assertEquals(wait, policy.request(100, automatic, browser, carlink, reported, saved))
                assertEquals(if (wait) 20_100L else null, policy.untilMillis)
            }
    }
    @Test fun waitHasAnExactDeadlineAndScreenOrStartNowReleasesIt() {
        val policy = CarFirstPolicy()
        fun request() { policy.request(100, true, true, false, false, false) }
        request()
        assertFalse(policy.ready(101, false))
        assertFalse(policy.ready(20_099, false))
        assertEquals(20_100L, policy.untilMillis)
        assertTrue(policy.ready(20_100, false))
        assertNull(policy.untilMillis)
        request()
        assertTrue(policy.ready(150, true))
        assertNull(policy.untilMillis)
        request()
        policy.cancel()
        assertTrue(policy.ready(101, false))
        assertNull(policy.untilMillis)
    }
    @Test fun wirelessOnlyStartsDoNotWaitWhileDeclarationsCannotReportACarScreen() {
        for (id in listOf("cast", "miracast")) {
            val memory = CarDisplayMemory()
            val caps = OutputCapabilities(LatencyClass.INTERACTIVE, listOf(DecoderSupport("avc1.64002a", true)),
                mapOf(ScreenId.MAIN to DisplayInfo(1920, 1080, 160, 60.0)), 0, emptySet(), emptySet(), false)
            memory.exclude(RecordingOutput(id, caps)).open(object : OutputHost {
                override fun capabilities(capabilities: OutputCapabilities) {
                    assertNull(memory.measuredDisplay(capabilities))
                }
                override fun control(event: ControlEvent) {}
                override fun stats(stats: OutputStats) {}
            })
            val settings = RelaySettings(ConnectionMode.USB, com.shilapi.xcertplay.orchestration.MfiTarget.LOCAL, "", "", "", null,
                autoStartOnBoot = false, autoStartWhenReady = true, localPreview = false, webPort = null,
                outputs = setOf(if (id == "cast") RelayOutput.DLNA else RelayOutput.MIRACAST))
            val policy = CarFirstPolicy()
            assertFalse(policy.request(0, true, settings.browserOutput, settings.carlinkOutput,
                memory.measuredDisplay(caps) != null, false))
            assertTrue(policy.ready(0, false))
        }
    }

    @Test fun anotherStartUsesItsOwnDeadline() {
        val policy = CarFirstPolicy()
        policy.request(0, true, false, true, false, false)
        policy.cancel()
        policy.request(50_000, true, false, true, false, false)
        assertEquals(70_000L, policy.untilMillis)
        assertFalse(policy.ready(50_001, false))
    }
}
