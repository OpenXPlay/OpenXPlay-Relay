// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-23
package org.openxplay.direlay.logs

import android.app.Application
import kotlinx.coroutines.CancellationException
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
    private val redactor = LogRedactor(LogSecrets(key = ByteArray(32)))
    private fun trace(file: File, vararg times: Long) {
        file.writeText(times.joinToString("\n", postfix = "\n") { at ->
            JSONObject().put("type", "Rtsp").put("atMillis", at).put("body", "wfd_video_formats: caps\npassword: export-secret-value").toString()
        })
    }
    @Test fun zipIncludesOnlyOverlappingTraceFilesAndRedactsEveryLine() = runBlocking {
        val traces = temporary.newFolder()
        trace(File(traces, "trace-overlap.jsonl"), now - 700_000, now - 100)
        trace(File(traces, "trace-old.jsonl"), now - 700_000)
        trace(File(traces, "trace-future.jsonl"), now + 100)
        File(traces, "media.ts").writeText("not trace data")
        LogSegmentStore(temporary.newFolder(), { now }).use { store ->
            val file = checkNotNull(LogExporter(temporary.newFolder(), store, redactor, { "App: Test" }, { now }, traceDirectory = { traces })
                .export(LogRange.LAST_10_MINUTES))
            ZipFile(file).use { zip ->
                assertEquals(setOf("relay.log", "info.txt", "carlink-trace/trace-overlap.jsonl"), zip.entries().asSequence().map { it.name }.toSet())
                val text = zip.getInputStream(zip.getEntry("carlink-trace/trace-overlap.jsonl")).bufferedReader().readText()
                assertEquals(2, text.lines().count { it.isNotEmpty() })
                assertTrue(text.contains("wfd_video_formats: caps")); assertTrue(text.contains("[secret#"))
                assertFalse(text.contains("export-secret-value"))
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
            val file = checkNotNull(LogExporter(temporary.newFolder(), store, redactor, { "App: Test" }, { now }).export(LogRange.LAST_24_HOURS))
            ZipFile(file).use { zip -> assertFalse(zip.entries().asSequence().any { it.name.startsWith("carlink-trace/") }) }
        }
    }
    @Test fun cancellationInsideTraceStreamingRemovesPartialZip() = runBlocking {
        val traces = temporary.newFolder()
        trace(File(traces, "trace-current.jsonl"), *LongArray(200) { now })
        val exports = temporary.newFolder()
        var lines = 0
        val cancelling = LogRedactor(LogSecrets(refresh = { if (++lines == 10) throw CancellationException("cancelled") }, key = ByteArray(32)))
        LogSegmentStore(temporary.newFolder(), { now }).use { store ->
            try {
                LogExporter(exports, store, cancelling, { "App: Test" }, { now }, traceDirectory = { traces }).export(LogRange.LAST_10_MINUTES)
                fail("Expected cancellation")
            } catch (_: CancellationException) { }
            assertTrue(exports.listFiles()!!.isEmpty())
        }
    }
    @Test fun activeFileSnapshotSkipsAnIncompleteLastRecord() = runBlocking {
        val traces = temporary.newFolder()
        val source = File(traces, "trace-current.jsonl")
        trace(source, now)
        source.appendText("{\"atMillis\":$now,\"password\":\"unfinished")
        LogSegmentStore(temporary.newFolder(), { now }).use { store ->
            val file = checkNotNull(LogExporter(temporary.newFolder(), store, redactor, { "App: Test" }, { now }, traceDirectory = { traces })
                .export(LogRange.LAST_10_MINUTES))
            ZipFile(file).use { zip ->
                val text = zip.getInputStream(zip.getEntry("carlink-trace/trace-current.jsonl")).bufferedReader().readText()
                assertEquals(1, text.lines().count { it.isNotEmpty() }); assertFalse(text.contains("unfinished"))
            }
        }
    }
}
