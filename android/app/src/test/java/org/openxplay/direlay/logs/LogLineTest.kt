// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-24
package org.openxplay.direlay.logs

import org.junit.Assert.*
import org.junit.Test
import java.time.LocalDateTime
import java.time.ZoneOffset
import java.time.format.DateTimeFormatter
import kotlin.random.Random

class LogLineTest {
    private val stamp = Regex("^(\\d{4}-\\d{2}-\\d{2} \\d{2}:\\d{2}:\\d{2}\\.\\d{3})(?:\\s+(Z|UTC|[+-]\\d{2}:?\\d{2}))?")
    private val format = DateTimeFormatter.ofPattern("yyyy-MM-dd HH:mm:ss.SSS")
    private fun referenceTimestamp(line: String): Long? {
        val match = stamp.find(line) ?: return null
        return runCatching {
            val zone = match.groupValues[2]
            LocalDateTime.parse(match.groupValues[1], format)
                .toInstant(if (zone.isEmpty() || zone == "UTC") ZoneOffset.UTC else ZoneOffset.of(zone)).toEpochMilli()
        }.getOrNull()
    }
    private fun assertMatchesReference(line: String) {
        assertEquals(line, referenceTimestamp(line), LogLine.timestamp(line))
    }

    @Test fun realLinesAndZoneVariantsMatchTheOriginalParser() {
        val time = "2026-10-24 12:34:56.789"
        listOf(
            "$time +0000  1234  5678 I Tag: msg",
            "2026-01-01 00:00:00.000 +0000  1  2 I Tag: msg",
            "2026-12-31 23:59:59.999 +0000  1  2 I Tag: msg",
            time, "${time}x", "${time}Z", "${time}+0800",
            "$time Z", "$time UTC", "$time +08:00", "$time +0800", "$time -0530",
            "$time\t+0000", "$time  +0000", "$time\u00a0+0800", "$time\u2003+0800",
            "$time +0000suffix", "$time +00:00", "$time -0000", "$time +1900"
        ).forEach(::assertMatchesReference)
    }
    @Test fun invalidDatesAndMalformedStampsMatchTheOriginalParser() {
        listOf(
            "2028-02-29 12:34:56.789", "2026-02-29 12:34:56.789", "2026-04-31 12:34:56.789",
            "2026-00-24 12:34:56.789", "2026-13-24 12:34:56.789", "2026-10-00 12:34:56.789",
            "2026-10-32 12:34:56.789", "0000-10-24 12:34:56.789", "0001-01-01 00:00:00.000",
            "9999-12-31 23:59:59.999", "2026-10-24 24:00:00.000", "2026-10-24 24:34:56.789",
            "2026-10-24 12:60:56.789", "2026-10-24 12:34:60.789",
            "٢٠٢٦-١٠-٢٤ ١٢:٣٤:٥٦.٧٨٩", "2026-10-24 12:34:56.78٩",
            "2026-10-24 12:34:56.78", "2026-10-24", "short", ""
        ).forEach { time ->
            assertMatchesReference(time)
            assertMatchesReference("$time +0000  1234  5678 I Tag: msg")
        }
    }
    @Test fun seededMutationsMatchTheOriginalParser() {
        val original = "2026-10-24 12:34:56.789 +0000  1234  5678 I Tag: msg"
        val random = Random(20261024)
        val replacements = "0123456789-: .+ZUTC\t\n\u00a0\u2003٠٩x"
        repeat(500) {
            val mutated = StringBuilder(original)
            repeat(random.nextInt(1, 5)) {
                when (random.nextInt(3)) {
                    0 -> mutated.setCharAt(random.nextInt(mutated.length), replacements[random.nextInt(replacements.length)])
                    1 -> mutated.deleteCharAt(random.nextInt(mutated.length))
                    else -> mutated.insert(random.nextInt(mutated.length + 1), replacements[random.nextInt(replacements.length)])
                }
            }
            assertMatchesReference(mutated.toString())
        }
    }
}
