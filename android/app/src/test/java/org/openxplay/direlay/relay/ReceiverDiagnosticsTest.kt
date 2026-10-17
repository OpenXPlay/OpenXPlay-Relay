// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-16
package org.openxplay.direlay.relay

import org.junit.Assert.*
import org.junit.Test

class ReceiverDiagnosticsTest {
    private var now = 0L
    private val diagnostics = ReceiverDiagnostics { now }
    private fun stats(fps: Int) = """{"type":"stats","screen":0,"renderedFps":$fps}"""

    @Test fun keepsLatestInfoAndARollingStatsWindowPerBrowser() {
        diagnostics.report("7", "receiver-info", """{"type":"receiver-info","refreshHz":60}""")
        repeat(ReceiverDiagnostics.MAX_SAMPLES + 5) { diagnostics.report("7", "stats", stats(it)); now += 100_000_000 }
        diagnostics.report("7", "receiver-info", """{"type":"receiver-info","refreshHz":59.9}""")
        now += 500_000_000
        val report = diagnostics.snapshot().single()
        assertEquals("7", report.client)
        assertEquals(0.5, report.idleSeconds, 0.0)
        assertTrue(report.info!!.contains("59.9"))
        assertEquals(ReceiverDiagnostics.MAX_SAMPLES, report.stats.size)
        assertEquals(stats(5), report.stats.first())
        assertEquals(stats(ReceiverDiagnostics.MAX_SAMPLES + 4), report.stats.last())
    }

    @Test fun boundsBrowsersAndForgetsSilentOnes() {
        repeat(ReceiverDiagnostics.MAX_CLIENTS + 3) { diagnostics.report("$it", "stats", stats(60)); now += 1_000_000_000 }
        now = 20_000_000_000; diagnostics.report("4", "stats", stats(59))
        now = 21_000_000_000
        assertEquals((3..10).map(Int::toString), diagnostics.snapshot().map { it.client })
        now = 70_000_000_000
        assertEquals(listOf("4"), diagnostics.snapshot().map { it.client })
        now = 81_000_000_000
        assertTrue(diagnostics.snapshot().isEmpty())
    }

    @Test fun ignoresUnknownMalformedAndOversizedReports() {
        diagnostics.report("1", "pong", "{}")
        diagnostics.report("", "stats", stats(60))
        diagnostics.report("x".repeat(65), "stats", stats(60))
        diagnostics.report("a\nb", "stats", stats(60))
        diagnostics.report("1", "receiver-info", "{\"pad\":\"" + "x".repeat(ReceiverDiagnostics.MAX_INFO_CHARS) + "\"}")
        diagnostics.report("1", "stats", "{\"pad\":\"" + "x".repeat(ReceiverDiagnostics.MAX_SAMPLE_CHARS) + "\"}")
        assertTrue(diagnostics.snapshot().isEmpty())
        diagnostics.report("2", "stats", stats(60))
        assertNull(diagnostics.snapshot().single().info)
        diagnostics.clear()
        assertTrue(diagnostics.snapshot().isEmpty())
    }
}
