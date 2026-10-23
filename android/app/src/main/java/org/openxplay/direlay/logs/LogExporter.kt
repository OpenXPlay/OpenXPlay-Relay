// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-22
package org.openxplay.direlay.logs

import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.ensureActive
import kotlinx.coroutines.withContext
import java.io.File
import java.time.Instant
import java.time.ZoneId
import java.time.format.DateTimeFormatter
import java.util.zip.Deflater
import java.util.zip.ZipEntry
import java.util.zip.ZipOutputStream

enum class LogRange(val millis: Long, val suffix: String) {
    LAST_10_MINUTES(600_000, "10m"), LAST_30_MINUTES(1_800_000, "30m"), LAST_24_HOURS(86_400_000, "24h")
}

class LogExporter(private val directory: File, private val store: LogSegmentStore, private val redactor: LogRedactor,
    private val metadata: () -> String, private val now: () -> Long = System::currentTimeMillis,
    private val zone: () -> ZoneId = ZoneId::systemDefault, private val flush: () -> Unit = store::flush,
    private val traceDirectory: () -> File? = { null }) {
    suspend fun export(range: LogRange): File? = withContext(Dispatchers.IO) {
        val job = currentCoroutineContext()
        job.ensureActive()
        directory.mkdirs()
        directory.listFiles().orEmpty().filter { it.isFile && it.name.endsWith(".zip") }.forEach { it.delete() }
        val time = now()
        val cutoff = time - range.millis
        val name = DateTimeFormatter.ofPattern("yyyyMMdd-HHmmss").withZone(zone()).format(Instant.ofEpochMilli(time))
        val file = File(directory, "openxplay-relay-logs-$name-${range.suffix}.zip")
        var complete = false
        try {
            flush()
            var count = 0L
            var first: Long? = null
            var last: Long? = null
            var include = false
            var earliest: Long? = null
            val stream = LogRedactorStream(redactor)
            store.snapshot().use { snapshot ->
                ZipOutputStream(file.outputStream().buffered()).use { zip ->
                    zip.setLevel(Deflater.BEST_COMPRESSION)
                    zip.putNextEntry(ZipEntry("relay.log"))
                    for (input in snapshot.streams) input.bufferedReader(Charsets.UTF_8).use { reader ->
                        while (true) {
                            job.ensureActive()
                            val line = reader.readLine() ?: break
                            val timestamp = LogLine.timestamp(line)
                            if (timestamp != null) {
                                include = timestamp >= cutoff && timestamp <= time
                                if (earliest == null || timestamp < earliest!!) earliest = timestamp
                            }
                            val cleaned = stream.redact(line)
                            if (!include) continue
                            if (timestamp != null) { if (first == null) first = timestamp; last = timestamp }
                            zip.write(cleaned.toByteArray(Charsets.UTF_8)); zip.write('\n'.code)
                            count++
                        }
                    }
                    zip.closeEntry()
                    val traces = traceDirectory()
                    if (traces != null) {
                        count += TraceLogExport.append(zip, traces, cutoff, time, redactor) { job.ensureActive() }
                    }
                    if (count > 0) {
                        job.ensureActive()
                        val info = buildString {
                            append(metadata().trimEnd()); append('\n')
                            append("Export time: ${Instant.ofEpochMilli(time)}\nDevice zone: ${zone().id}\n")
                            append("Selected range: ${range.name}\nFirst line: ${first?.let(Instant::ofEpochMilli)}\n")
                            append("Last line: ${last?.let(Instant::ofEpochMilli)}\n")
                            append("Private values (passwords, keys, phone numbers, names, messages, media and navigation text, precise location) are replaced by tokens; the same value gives the same token on this phone.\n")
                            if ((earliest ?: Long.MAX_VALUE) > cutoff) append("Logs start after the cut-off: recording was unavailable or older segments were removed by the 64 MiB storage cap or retention.\n")
                        }
                        zip.putNextEntry(ZipEntry("info.txt"))
                        zip.write(redactor.redact(info).toByteArray(Charsets.UTF_8))
                        zip.closeEntry()
                    }
                }
            }
            job.ensureActive()
            complete = count > 0
            if (complete) file else null
        } finally { if (!complete) file.delete() }
    }
}
