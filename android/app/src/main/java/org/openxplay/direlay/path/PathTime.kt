// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-25
package org.openxplay.direlay.path

import java.time.Instant
import java.time.ZoneId
import java.time.format.DateTimeFormatter
import java.util.Locale

fun countdownSeconds(deadlineMillis: Long, nowMillis: Long): Long {
    val remaining = (deadlineMillis - nowMillis).coerceAtLeast(0)
    return remaining / 1000 + if (remaining % 1000 > 0) 1 else 0
}

fun formatDiagnostic(line: String, version: String, nowMillis: Long, zone: ZoneId = ZoneId.systemDefault()): String =
    "$line | $version ${DateTimeFormatter.ofPattern("HH:mm:ss", Locale.ROOT).withZone(zone).format(Instant.ofEpochMilli(nowMillis))}"
