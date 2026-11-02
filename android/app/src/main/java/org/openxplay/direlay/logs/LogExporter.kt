// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-22
package org.openxplay.direlay.logs

import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.ensureActive
import kotlinx.coroutines.withContext
import java.io.BufferedOutputStream
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

class LogExporter(private val directory: File, private val store: LogSegmentStore,
    private val metadata: () -> String, private val now: () -> Long = System::currentTimeMillis,
    private val zone: () -> ZoneId = ZoneId::systemDefault, private val flush: () -> Unit = store::flush,
    private val traceDirectory: () -> File? = { null }, private val journal: TimelineJournal? = null,
    private val probe: () -> LogcatProbe.Result = { LogcatProbe.Result(LogcatProbe.State.NOT_STARTED, emptySet(), emptyMap()) },
    private val timelineDropped: () -> Long = { 0 }, private val recorderFailures: () -> Long = { 0 }) {
    @Volatile var captureIncomplete = false
        private set
    private class Lines {
        var count = 0L
        var first: Long? = null
        var last: Long? = null
        var earliest: Long? = null
    }
    private fun append(zip: ZipOutputStream, snapshot: LogSnapshot, entry: String, cutoff: Long, end: Long,
        checkCancelled: () -> Unit): Lines {
        val lines = Lines()
        var include = false
        zip.putNextEntry(ZipEntry(entry))
        val output = BufferedOutputStream(zip, 65_536)
        for (input in snapshot.streams) input.bufferedReader(Charsets.UTF_8).use { reader ->
            while (true) {
                checkCancelled()
                val line = reader.readLine() ?: break
                val at = LogLine.timestamp(line)
                if (at != null) {
                    include = at >= cutoff && at <= end
                    lines.earliest = minOf(lines.earliest ?: at, at)
                }
                if (!include) continue
                if (at != null) {
                    lines.first = minOf(lines.first ?: at, at)
                    lines.last = maxOf(lines.last ?: at, at)
                }
                output.write(line.toByteArray(Charsets.UTF_8)); output.write('\n'.code)
                lines.count++
            }
        }
        output.flush(); zip.closeEntry()
        return lines
    }
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
            store.snapshot(cutoff).use { snapshot ->
                ZipOutputStream(file.outputStream().buffered()).use { zip ->
                    zip.setLevel(Deflater.BEST_COMPRESSION)
                    val captured = append(zip, snapshot, "relay.log", cutoff, time) { job.ensureActive() }
                    count = captured.count
                    var journalUnavailable = false
                    val journalSnapshot = try { journal?.snapshot(cutoff) } catch (_: Exception) { journalUnavailable = true; null }
                    var journalHistoryMissing = false
                    val journalLines = journalSnapshot?.use {
                        append(zip, it, "timeline.log", cutoff, time) { job.ensureActive() }.also { lines ->
                            journalHistoryMissing = !it.skippedEarlier && (lines.earliest ?: Long.MAX_VALUE) > cutoff
                        }
                    } ?: Lines()
                    count += journalLines.count
                    val result = probe()
                    val dropped = timelineDropped()
                    val failedWrites = journal?.failedWrites ?: 0
                    val captureFailures = recorderFailures()
                    val historyMissing = !snapshot.skippedEarlier && (captured.earliest ?: Long.MAX_VALUE) > cutoff
                    captureIncomplete = result.incomplete || captureFailures > 0 || dropped > 0 || failedWrites > 0 || journalUnavailable ||
                        historyMissing || journalHistoryMissing
                    val traces = traceDirectory()
                    if (traces != null) {
                        count += TraceLogExport.append(zip, traces, cutoff, time) { job.ensureActive() }
                    }
                    if (count > 0) {
                        job.ensureActive()
                        val info = buildString {
                            append(metadata().trimEnd()); append('\n')
                            append("Export time: ${Instant.ofEpochMilli(time)}\nDevice zone: ${zone().id}\n")
                            append("Selected range: ${range.name}\nFirst line: ${captured.first?.let(Instant::ofEpochMilli)}\n")
                            append("Last line: ${captured.last?.let(Instant::ofEpochMilli)}\n")
                            append("Private values (passwords, keys, phone numbers, names, messages, media and navigation text, precise location) were replaced by tokens when the logs were recorded; the same value gives the same token on this phone.\n")
                            append("\nCapture completeness\n")
                            append("Logcat capture: ${captured.count} lines (relay.log; includes recorder notes)\n")
                            append("Timeline journal: ${journalLines.count} lines; first ${journalLines.first?.let(Instant::ofEpochMilli) ?: "none"}; last ${journalLines.last?.let(Instant::ofEpochMilli) ?: "none"}\n")
                            append("Timeline worker dropped: $dropped; journal failed writes: $failedWrites (current process)\n")
                            append("Recorder redaction/storage failures or skipped writes: $captureFailures (current process)\n")
                            append(result.summary())
                            if (journal == null) append("Timeline journal not configured.\n")
                            if (journalUnavailable) append("Timeline journal unavailable during export.\n")
                            if (journalHistoryMissing) append("Journal starts after the cut-off: recording was unavailable or older segments were removed by its size cap or retention.\n")
                            if (captureIncomplete) append("Capture is incomplete; absence of events does not establish car connectivity.\n")
                            else append("No capture loss identified; the probe does not guarantee every event was retained.\n")
                            if (historyMissing) append("Logs start after the cut-off: recording was unavailable or older segments were removed by the 64 MiB storage cap or retention.\n")
                        }
                        zip.putNextEntry(ZipEntry("info.txt"))
                        zip.write(info.toByteArray(Charsets.UTF_8))
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
