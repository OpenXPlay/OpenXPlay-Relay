// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-25
package org.openxplay.direlay.path

import org.junit.Assert.*
import org.junit.Test
import java.time.Instant
import java.time.ZoneId

class PathTimeTest {
    @Test fun diagnosticDisplayAndCopyKeepTheFullFormat() {
        val now = Instant.parse("2026-10-25T14:32:05Z").toEpochMilli()
        val line = "R MODE✓ ID✓ | I JOIN✗ | E first_tcp_timeout 14:31:58"
        assertEquals("$line | 0.1.6 14:32:05", formatDiagnostic(line, "0.1.6", now, ZoneId.of("UTC")))
        assertEquals("$line | 0.1.6 22:32:05", formatDiagnostic(line, "0.1.6", now, ZoneId.of("Asia/Shanghai")))
    }
    @Test fun countdownUsesTheOriginalCeilingAndClampsAtZero() {
        for ((remaining, seconds) in listOf(20000L to 20L, 19999L to 20L, 1001L to 2L,
            1000L to 1L, 1L to 1L, 0L to 0L, -1L to 0L)) {
            assertEquals(seconds, countdownSeconds(100000L + remaining, 100000L))
        }
    }
}
