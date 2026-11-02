// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-11-01
package org.openxplay.direlay.logs

import java.io.Closeable
import java.io.File
import java.time.Instant
import java.time.ZoneOffset
import java.time.format.DateTimeFormatter
import java.util.Locale

/** Opened on the connection worker, never the event caller. Only ConnectionLog's cleaned text enters here. */
class TimelineJournal(private val directory: () -> File, private val now: () -> Long = System::currentTimeMillis,
    private val cap: Long = 4L * 1024 * 1024) : Closeable {
    private var store: LogSegmentStore? = null
    @Volatile var failedWrites = 0L
        private set
    private fun store(): LogSegmentStore {
        store?.let { return it }
        val target = directory()
        check(!java.nio.file.Files.isSymbolicLink(target.toPath()) && (target.isDirectory || target.mkdirs())) { "Journal directory unavailable" }
        return LogSegmentStore(target, now, cap).also { it.prune(); store = it }
    }
    @Synchronized internal fun append(tag: String, redacted: String, at: Long) {
        try {
            // Synthetic UTC headers allow the same range selection as logcat; the message is identical.
            store().append("${format.format(Instant.ofEpochMilli(at))} +0000 0 0 I $tag: $redacted")
            store().flush()
        } catch (_: Exception) {
            failedWrites++
            runCatching { store?.close() }; store = null
        }
    }
    @Synchronized internal fun maintenance() { store().maintenance() }
    @Synchronized internal fun snapshot(since: Long): LogSnapshot = store().snapshot(since)
    @Synchronized override fun close() { store?.close(); store = null }
    private companion object {
        val format = DateTimeFormatter.ofPattern("yyyy-MM-dd HH:mm:ss.SSS", Locale.ROOT).withZone(ZoneOffset.UTC)
    }
}
