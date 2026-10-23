// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-22
package org.openxplay.direlay.logs

import java.io.BufferedWriter
import java.io.Closeable
import java.io.File
import java.io.FileInputStream
import java.io.FilterInputStream
import java.io.InputStream
import java.time.Instant
import java.time.ZoneOffset
import java.time.format.DateTimeFormatter

class LogSegmentStore(private val directory: File, private val now: () -> Long = System::currentTimeMillis,
    private val cap: Long = 64L * 1024 * 1024) : Closeable {
    private var writer: BufferedWriter? = null
    private var current: File? = null
    private var currentBytes = 0L
    private var totalBytes = 0L
    private var lastPrune = Long.MIN_VALUE
    @Volatile var lastTimestamp: Long? = null
        private set
    init {
        runCatching {
            directory.mkdirs()
            lastTimestamp = segments().asReversed().firstNotNullOfOrNull(::resumeTimestamp)
            totalBytes = segments().sumOf { it.length() }
        }
    }
    @Synchronized fun append(redacted: String) {
        val timestamp = LogLine.timestamp(redacted)
        val bucket = hour.format(Instant.ofEpochMilli(timestamp ?: lastTimestamp ?: now()))
        val bytes = redacted.toByteArray(Charsets.UTF_8).size.toLong() + 1
        if (bytes > cap) return
        var opened = false
        if (current?.name?.startsWith("relay-$bucket") != true || currentBytes + bytes > cap) {
            writer?.close(); writer = null
            check(!java.nio.file.Files.isSymbolicLink(directory.toPath()) && (directory.isDirectory || directory.mkdirs())) { "Log directory unavailable" }
            val sameHour = segments().filter { it.name.startsWith("relay-$bucket") }
            val latest = sameHour.lastOrNull()
            val next = if (latest != null && latest.length() + bytes <= cap) latest else {
                val suffix = sameHour.maxOfOrNull { index(it) }?.plus(1) ?: 1
                File(directory, "relay-$bucket" + if (suffix == 1) ".log" else "-$suffix.log")
            }
            current = next
            writer = next.bufferedWriterAppend()
            currentBytes = next.length()
            opened = true
        }
        checkNotNull(writer).apply { write(redacted); newLine() }
        currentBytes += bytes
        totalBytes += bytes
        timestamp?.let { lastTimestamp = it }
        if (opened || totalBytes > cap || lastPrune == Long.MIN_VALUE || now() - lastPrune >= 600_000) prune()
    }
    private fun File.bufferedWriterAppend() = java.io.FileOutputStream(this, true).bufferedWriter(Charsets.UTF_8)
    @Synchronized fun flush() { writer?.flush() }
    @Synchronized fun maintenance() {
        flush()
        if (lastPrune == Long.MIN_VALUE || now() - lastPrune >= 600_000) prune()
    }
    @Synchronized fun prune() {
        flush()
        lastPrune = now()
        val files = segments().toMutableList()
        files.filter { file ->
            val start = segmentHour(file) ?: return@filter false
            start + 3_600_000 < now() - 25 * 3_600_000L
        }.forEach { file ->
            if (file == current) { writer?.close(); writer = null; current = null; currentBytes = 0 }
            if (file.delete()) files.remove(file)
        }
        var total = files.sumOf { if (it == current) currentBytes else it.length() }
        for (file in files) {
            if (total <= cap) break
            if (file == current) continue
            val size = file.length()
            if (file.delete()) total -= size
        }
        totalBytes = total
    }
    @Synchronized fun snapshot(): LogSnapshot {
        flush()
        val opened = mutableListOf<InputStream>()
        try {
            for (file in segments()) {
                val length = file.length()
                opened += object : FilterInputStream(FileInputStream(file)) {
                    var remaining = length
                    override fun read(): Int {
                        if (remaining <= 0) return -1
                        val value = super.read()
                        if (value >= 0) remaining--
                        return value
                    }
                    override fun read(bytes: ByteArray, offset: Int, count: Int): Int {
                        if (remaining <= 0) return -1
                        val size = super.read(bytes, offset, minOf(count.toLong(), remaining).toInt())
                        if (size > 0) remaining -= size
                        return size
                    }
                }
            }
            return LogSnapshot(opened)
        } catch (failure: Exception) { opened.forEach { runCatching { it.close() } }; throw failure }
    }
    @Synchronized override fun close() {
        try { writer?.close() } finally { writer = null; current = null }
    }
    private fun resumeTimestamp(file: File): Long? = java.io.RandomAccessFile(file, "r").use { input ->
        var position = input.length()
        var tail = ""
        val buffer = ByteArray(8192)
        while (position > 0) {
            val count = minOf(position, buffer.size.toLong()).toInt()
            position -= count
            input.seek(position); input.readFully(buffer, 0, count)
            val lines = (String(buffer, 0, count, Charsets.ISO_8859_1) + tail).split('\n')
            for (index in lines.lastIndex downTo 1) LogLine.timestamp(lines[index])?.let { return@use it }
            tail = lines.first()
        }
        LogLine.timestamp(tail)
    }
    private fun segments(): List<File> = if (java.nio.file.Files.isSymbolicLink(directory.toPath())) emptyList() else directory.listFiles().orEmpty().filter {
        it.isFile && !java.nio.file.Files.isSymbolicLink(it.toPath()) && name.matches(it.name)
    }.sortedWith(compareBy<File> { it.name.substring(6, 17) }.thenBy { index(it) })
    private fun index(file: File) = name.matchEntire(file.name)?.groupValues?.get(2)?.toIntOrNull() ?: 1
    private fun segmentHour(file: File): Long? = runCatching {
        java.time.LocalDateTime.parse(file.name.substring(6, 17), hourLocal).toInstant(ZoneOffset.UTC).toEpochMilli()
    }.getOrNull()
    companion object {
        private val name = Regex("relay-(\\d{8}-\\d{2})(?:-(\\d+))?\\.log")
        private val hour = DateTimeFormatter.ofPattern("yyyyMMdd-HH").withZone(ZoneOffset.UTC)
        private val hourLocal = DateTimeFormatter.ofPattern("yyyyMMdd-HH").let {
            java.time.format.DateTimeFormatterBuilder().append(it).parseDefaulting(java.time.temporal.ChronoField.MINUTE_OF_HOUR, 0).toFormatter()
        }
    }
}

class LogSnapshot(val streams: List<InputStream>) : Closeable {
    override fun close() { streams.forEach { runCatching { it.close() } } }
}
