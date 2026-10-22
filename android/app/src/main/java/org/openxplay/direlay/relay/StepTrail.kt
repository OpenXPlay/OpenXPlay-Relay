// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-22
package org.openxplay.direlay.relay

import java.time.Instant
import java.time.ZoneId
import java.time.format.DateTimeFormatter
import java.util.Collections
import java.util.Locale

/** Bounded facts for one attempt; snapshots cannot change under the displayed error. */
class StepTrail(private val zone: ZoneId = ZoneId.systemDefault()) {
    private val lines = ArrayList<String>()
    private var startNanos = 0L
    private var omitted = 0
    @Synchronized fun record(line: String, startsAttempt: Boolean, wallMillis: Long, nanos: Long) {
        if (startsAttempt) clear()
        val prefix = if (lines.isEmpty()) {
            startNanos = nanos
            DateTimeFormatter.ofPattern("HH:mm:ss.SSS", Locale.ROOT).withZone(zone)
                .format(Instant.ofEpochMilli(wallMillis)) + " "
        } else String.format(Locale.ROOT, "+%.3fs ", (nanos - startNanos).coerceAtLeast(0) / 1_000_000_000.0)
        lines.add((prefix + line).take(200))
        if (lines.size > 80) {
            if (omitted == 0) {
                lines.removeAt(1)
                lines.removeAt(1)
                omitted = 2
                lines.add(1, "")
            } else { lines.removeAt(2); omitted++ }
            lines[1] = "… $omitted earlier lines omitted"
        }
    }
    @Synchronized fun snapshot(): List<String> = Collections.unmodifiableList(ArrayList(lines))
    @Synchronized fun clear() { lines.clear(); startNanos = 0; omitted = 0 }
}
