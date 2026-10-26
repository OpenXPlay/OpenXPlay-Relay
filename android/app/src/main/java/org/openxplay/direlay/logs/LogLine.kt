// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-22
package org.openxplay.direlay.logs

import java.time.LocalDate
import java.time.LocalDateTime
import java.time.YearMonth
import java.time.ZoneOffset
import java.time.format.DateTimeFormatter

object LogLine {
    private val stamp = Regex("^(\\d{4}-\\d{2}-\\d{2} \\d{2}:\\d{2}:\\d{2}\\.\\d{3})(?:\\s+(Z|UTC|[+-]\\d{2}:?\\d{2}))?")
    private val header = Regex("^\\d{4}-\\d{2}-\\d{2} \\d{2}:\\d{2}:\\d{2}\\.\\d{3}(?:\\s+(?:Z|UTC|[+-]\\d{2}:?\\d{2}))?\\s+\\d+\\s+\\d+\\s+[VDIWEFAS]\\s+[^:]+: ?")
    private val format = DateTimeFormatter.ofPattern("yyyy-MM-dd HH:mm:ss.SSS")
    fun timestamp(line: String): Long? {
        fastTimestamp(line)?.let { return it }
        val match = stamp.find(line) ?: return null
        return runCatching {
            val zone = match.groupValues[2]
            LocalDateTime.parse(match.groupValues[1], format)
                .toInstant(if (zone.isEmpty() || zone == "UTC") ZoneOffset.UTC else ZoneOffset.of(zone)).toEpochMilli()
        }.getOrNull()
    }
    private fun fastTimestamp(line: String): Long? {
        if (line.length < 23 || line[4] != '-' || line[7] != '-' || line[10] != ' ' ||
            line[13] != ':' || line[16] != ':' || line[19] != '.') return null
        if (line.length > 23 && line[23].isWhitespace() && !line.startsWith(" +0000", 23)) return null
        val year = digits(line, 0, 4)
        val month = digits(line, 5, 2)
        val day = digits(line, 8, 2)
        val hour = digits(line, 11, 2)
        val minute = digits(line, 14, 2)
        val second = digits(line, 17, 2)
        val millis = digits(line, 20, 3)
        // The formatter uses year-of-era (yyyy), which rejects year zero.
        if (year < 1 || month !in 1..12 || day < 1 || hour !in 0..23 ||
            minute !in 0..59 || second !in 0..59 || millis < 0) return null
        if (day > YearMonth.of(year, month).lengthOfMonth()) return null
        return LocalDate.of(year, month, day).toEpochDay() * 86_400_000L +
            hour * 3_600_000L + minute * 60_000L + second * 1000L + millis
    }
    private fun digits(line: String, from: Int, count: Int): Int {
        var value = 0
        for (index in from until from + count) {
            val digit = line[index]
            if (digit !in '0'..'9') return -1
            value = value * 10 + (digit - '0')
        }
        return value
    }
    fun header(line: String): String = header.find(line)?.value.orEmpty()
    fun message(line: String): String = line.substring(header(line).length)
}
