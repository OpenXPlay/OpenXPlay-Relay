// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-23
package org.openxplay.direlay.logs

import android.app.Application
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.cancel
import kotlinx.coroutines.launch
import kotlinx.coroutines.runBlocking
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import java.io.File
import java.util.zip.ZipFile

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [33], manifest = Config.NONE, application = Application::class)
class TraceLogExportTest {
    @get:Rule val temporary = TemporaryFolder()
    private val now = 1_792_727_100_000L
    private fun trace(file: File, vararg times: Long) {
        file.writeText(times.joinToString("\n", postfix = "\n") { at ->
            JSONObject().put("type", "Rtsp").put("atMillis", at).put("body", "wfd_video_formats: caps\npassword: [secret#0a1b2c]").toString()
        })
    }
    @Test fun zipIncludesOnlyOverlappingTraceFilesVerbatim() = runBlocking {
        val traces = temporary.newFolder()
        val overlap = File(traces, "trace-overlap.jsonl")
        trace(overlap, now - 700_000, now - 100)
        trace(File(traces, "trace-old.jsonl"), now - 700_000)
        trace(File(traces, "trace-future.jsonl"), now + 100)
        File(traces, "media.ts").writeText("not trace data")
        LogSegmentStore(temporary.newFolder(), { now }).use { store ->
            val file = checkNotNull(LogExporter(temporary.newFolder(), store, { "App: Test" }, { now }, traceDirectory = { traces })
                .export(LogRange.LAST_10_MINUTES))
            ZipFile(file).use { zip ->
                assertEquals(setOf("relay.log", "info.txt", "carlink-trace/trace-overlap.jsonl"), zip.entries().asSequence().map { it.name }.toSet())
                assertArrayEquals(overlap.readBytes(), zip.getInputStream(zip.getEntry("carlink-trace/trace-overlap.jsonl")).readBytes())
            }
        }
    }
    @Test fun filesWithAnyCompleteLineWithoutANumericTimestampAreSkippedEntirely() = runBlocking {
        val traces = temporary.newFolder()
        val current = File(traces, "trace-current.jsonl")
        trace(current, now)
        listOf("{\"type\":\"Rtsp\",\"body\":\"raw\"}", "{\"atMillis\":\"$now\"}", "{\"atMillis\":null}", "not JSON").forEachIndexed { index, line ->
            val legacy = File(traces, "trace-$now-$index.jsonl")
            trace(legacy, now)
            legacy.appendText(line + "\n")
            legacy.setLastModified(now)
        }
        LogSegmentStore(temporary.newFolder(), { now }).use { store ->
            val file = checkNotNull(LogExporter(temporary.newFolder(), store, { "App: Test" }, { now }, traceDirectory = { traces })
                .export(LogRange.LAST_10_MINUTES))
            ZipFile(file).use { zip ->
                assertEquals(setOf("relay.log", "info.txt", "carlink-trace/trace-current.jsonl"), zip.entries().asSequence().map { it.name }.toSet())
                assertArrayEquals(current.readBytes(), zip.getInputStream(zip.getEntry("carlink-trace/trace-current.jsonl")).readBytes())
            }
        }
    }
    @Test fun baselineNeverReadsOrIncludesTraces() = runBlocking {
        val traces = temporary.newFolder()
        trace(File(traces, "trace-current.jsonl"), now)
        LogSegmentStore(temporary.newFolder(), { now }).use { store ->
            val stamp = java.time.format.DateTimeFormatter.ofPattern("yyyy-MM-dd HH:mm:ss.SSS")
                .withZone(java.time.ZoneOffset.UTC).format(java.time.Instant.ofEpochMilli(now))
            store.append("$stamp +0000  1  2 I Test: baseline")
            // No traceDirectory is supplied when the developer switch is off.
            val file = checkNotNull(LogExporter(temporary.newFolder(), store, { "App: Test" }, { now }).export(LogRange.LAST_24_HOURS))
            ZipFile(file).use { zip -> assertFalse(zip.entries().asSequence().any { it.name.startsWith("carlink-trace/") }) }
        }
    }
    @Test fun cancellationInsideTraceStreamingRemovesPartialZip() = runBlocking {
        val traces = temporary.newFolder()
        trace(File(traces, "trace-current.jsonl"), *LongArray(200) { now })
        val exports = temporary.newFolder()
        LogSegmentStore(temporary.newFolder(), { now }).use { store ->
            val worker = launch {
                val parent = coroutineContext
                try {
                    LogExporter(exports, store, { "App: Test" }, { now }, traceDirectory = { parent.cancel(); traces })
                        .export(LogRange.LAST_10_MINUTES)
                    fail("Expected cancellation")
                } catch (_: CancellationException) { }
            }
            worker.join()
            assertTrue(exports.listFiles()!!.isEmpty())
        }
    }
    @Test fun activeFileSnapshotSkipsAnIncompleteLastRecord() = runBlocking {
        val traces = temporary.newFolder()
        val source = File(traces, "trace-current.jsonl")
        trace(source, now)
        val complete = source.readBytes()
        source.appendText("{\"atMillis\":$now,\"password\":\"unfinished")
        LogSegmentStore(temporary.newFolder(), { now }).use { store ->
            val file = checkNotNull(LogExporter(temporary.newFolder(), store, { "App: Test" }, { now }, traceDirectory = { traces })
                .export(LogRange.LAST_10_MINUTES))
            ZipFile(file).use { zip ->
                assertArrayEquals(complete, zip.getInputStream(zip.getEntry("carlink-trace/trace-current.jsonl")).readBytes())
            }
        }
    }
}
