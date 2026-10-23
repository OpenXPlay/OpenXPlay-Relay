// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-23
package org.openxplay.direlay.logs

import java.io.BufferedReader
import java.io.File
import java.io.FileInputStream
import java.io.FilterInputStream
import java.nio.ByteBuffer
import java.nio.file.Files
import java.util.zip.ZipEntry
import java.util.zip.ZipOutputStream
import org.json.JSONObject

/** Two bounded streaming passes over an open inode; rotation cannot replace the file under us. */
object TraceLogExport {
    fun append(zip: ZipOutputStream, directory: File, cutoff: Long, end: Long, redactor: LogRedactor,
        checkCancelled: () -> Unit): Long {
        if (Files.isSymbolicLink(directory.toPath())) return 0
        var written = 0L
        val files = directory.listFiles().orEmpty().filter {
            it.isFile && it.name.startsWith("trace-") && it.name.endsWith(".jsonl") && !Files.isSymbolicLink(it.toPath())
        }.sortedBy { it.name }
        for (file in files) {
            checkCancelled()
            val input = try { FileInputStream(file) } catch (_: java.io.FileNotFoundException) { continue }
            input.use {
                val limit = input.channel.size()
                if (limit == 0L) return@use
                val tail = ByteBuffer.allocate(1)
                input.channel.position(limit - 1); input.channel.read(tail)
                val terminated = tail.array()[0] == '\n'.code.toByte()
                input.channel.position(0)
                var first: Long? = null
                var last: Long? = null
                // Old traces lack wall timestamps. Their filename start and mtime bound the span.
                val legacyStart = file.name.removePrefix("trace-").substringBefore('-').toLongOrNull()
                var legacy = false
                lines(reader(input, limit), terminated, checkCancelled) { line ->
                    val json = runCatching { JSONObject(line) }.getOrNull()
                    if (json != null) {
                        val at = runCatching { json.getLong("atMillis") }.getOrNull()
                        if (at == null) legacy = true
                        else {
                            first = minOf(first ?: at, at); last = maxOf(last ?: at, at)
                        }
                    }
                }
                if (legacy) {
                    legacyStart?.let { first = minOf(first ?: it, it) }
                    last = maxOf(last ?: file.lastModified(), file.lastModified())
                }
                val startTime = first ?: return@use
                if (startTime > end || (last ?: Long.MIN_VALUE) < cutoff) return@use
                zip.putNextEntry(ZipEntry("carlink-trace/${file.name}"))
                input.channel.position(0)
                val stream = LogRedactorStream(redactor)
                lines(reader(input, limit), terminated, checkCancelled) { line ->
                    val cleaned = stream.redact(TracePrivacy.redact(line, redactor))
                    zip.write(cleaned.toByteArray(Charsets.UTF_8)); zip.write('\n'.code); written++
                }
                zip.closeEntry()
            }
        }
        return written
    }
    private fun reader(input: FileInputStream, length: Long): BufferedReader = object : FilterInputStream(input) {
        var remaining = length
        override fun read(): Int {
            if (remaining <= 0) return -1
            return super.read().also { if (it >= 0) remaining-- }
        }
        override fun read(bytes: ByteArray, offset: Int, count: Int): Int {
            if (remaining <= 0) return -1
            return super.read(bytes, offset, minOf(count.toLong(), remaining).toInt()).also { if (it > 0) remaining -= it }
        }
    }.bufferedReader(Charsets.UTF_8)

    private inline fun lines(reader: BufferedReader, terminated: Boolean, checkCancelled: () -> Unit, action: (String) -> Unit) {
        var line = reader.readLine() ?: return
        while (true) {
            checkCancelled()
            val next = reader.readLine()
            // A concurrently written final JSON record is not part of this snapshot until its newline.
            if (next != null || terminated) action(line)
            if (next == null) return
            line = next
        }
    }
}
