// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-22
package org.openxplay.direlay.logs

import java.time.LocalDateTime
import java.time.ZoneOffset
import java.time.format.DateTimeFormatter

object LogLine {
    private val stamp = Regex("^(\\d{4}-\\d{2}-\\d{2} \\d{2}:\\d{2}:\\d{2}\\.\\d{3})(?:\\s+(Z|UTC|[+-]\\d{2}:?\\d{2}))?")
    private val header = Regex("^\\d{4}-\\d{2}-\\d{2} \\d{2}:\\d{2}:\\d{2}\\.\\d{3}(?:\\s+(?:Z|UTC|[+-]\\d{2}:?\\d{2}))?\\s+\\d+\\s+\\d+\\s+[VDIWEFAS]\\s+[^:]+: ?")
    private val format = DateTimeFormatter.ofPattern("yyyy-MM-dd HH:mm:ss.SSS")
    fun timestamp(line: String): Long? {
        val match = stamp.find(line) ?: return null
        return runCatching {
            val zone = match.groupValues[2]
            LocalDateTime.parse(match.groupValues[1], format)
                .toInstant(if (zone.isEmpty() || zone == "UTC") ZoneOffset.UTC else ZoneOffset.of(zone)).toEpochMilli()
        }.getOrNull()
    }
    fun header(line: String): String = header.find(line)?.value.orEmpty()
    fun message(line: String): String = line.substring(header(line).length)
}
