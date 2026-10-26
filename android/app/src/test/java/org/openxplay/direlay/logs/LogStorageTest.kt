// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-22
package org.openxplay.direlay.logs

import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.cancel
import kotlinx.coroutines.launch
import kotlinx.coroutines.runBlocking
import org.junit.Assert.*
import org.junit.Assume.assumeFalse
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder
import java.io.File
import java.time.Instant
import java.time.ZoneId
import java.time.ZoneOffset
import java.time.format.DateTimeFormatter
import java.util.zip.ZipFile

class LogStorageTest {
    @get:Rule val temporary = TemporaryFolder()
    private val time = Instant.parse("2026-10-23T05:05:00Z").toEpochMilli()
    private val stamp = DateTimeFormatter.ofPattern("yyyy-MM-dd HH:mm:ss.SSS").withZone(ZoneOffset.UTC)
    private fun line(at: Long, text: String) = "${stamp.format(Instant.ofEpochMilli(at))} +0000  1  2 I Test: $text"
    private fun contents(store: LogSegmentStore): String = store.snapshot().use { snapshot ->
        snapshot.streams.joinToString("") { it.bufferedReader().readText() }
    }

    @Test fun timestampsAllowMissingZoneAndExplicitUtcOffsets() {
        assertEquals(time, LogLine.timestamp(line(time, "normal")))
        assertEquals(time, LogLine.timestamp(line(time, "normal").replace(" +0000", "")))
        assertEquals(time, LogLine.timestamp(line(time, "normal").replace(" +0000", " UTC")))
        assertEquals(time - 3_600_000, LogLine.timestamp(line(time, "normal").replace(" +0000", " +0100")))
        assertNull(LogLine.timestamp("--------- beginning of main"))
    }
    @Test fun hoursComeFromLinesAndReopeningNeverTruncatesTheSegment() {
        val directory = temporary.newFolder()
        LogSegmentStore(directory, { time }).use { store ->
            store.append(line(time - 3_600_000, "earlier")); store.append(line(time, "now")); store.flush()
            assertEquals(setOf("relay-20261023-04.log", "relay-20261023-05.log"), directory.list()!!.toSet())
        }
        LogSegmentStore(directory, { time }).use { store ->
            assertEquals(time, store.lastTimestamp)
            store.append("no timestamp")
            assertTrue(contents(store).contains("now\nno timestamp\n"))
        }
    }
    @Test fun unstampedContinuationStaysInPreviousLinesHourAcrossTheWallClockBoundary() {
        val directory = temporary.newFolder()
        LogSegmentStore(directory, { time }).use { store ->
            store.append(line(time - 3_600_000, "previous hour"))
            store.append("continuation")
            store.flush()
            assertEquals(listOf("relay-20261023-04.log"), directory.list()!!.toList())
            assertTrue(File(directory, "relay-20261023-04.log").readText().endsWith("previous hour\ncontinuation\n"))
        }
    }
    @Test fun ageRetentionUsesHourEndAndNeverDeletesUnrelatedFiles() {
        val directory = temporary.newFolder()
        val old = File(directory, "relay-20261022-03.log").apply { writeText(line(time - 26 * 3_600_000L, "old") + "\n") }
        val boundary = File(directory, "relay-20261022-04.log").apply { writeText("keep") }
        val retained = File(directory, "relay-20261022-05.log").apply { writeText("keep") }
        val unrelated = File(directory, "other.txt").apply { writeText("keep") }
        var now = time - 300_001
        LogSegmentStore(directory, { now }).use { store ->
            store.prune()
            assertTrue(boundary.exists())
            now++
            store.prune()
            assertTrue(boundary.exists())
            now++
            store.prune()
        }
        assertFalse(old.exists())
        assertFalse(boundary.exists())
        assertTrue(retained.exists())
        assertTrue(unrelated.exists())
        assertTrue(directory.isDirectory)
    }
    @Test fun sizeCapDeletesOldestSegmentsAndRollsOverAnOversizedHour() {
        val directory = temporary.newFolder()
        val first = line(time, "a".repeat(30))
        val cap = (first.toByteArray().size + 1L) * 2
        LogSegmentStore(directory, { time }, cap).use { store ->
            store.append(first); store.append(first); store.append(line(time + 1, "b".repeat(30)))
            store.flush()
            assertTrue(directory.listFiles()!!.sumOf { it.length() } <= cap)
            assertEquals(listOf("relay-20261023-05-2.log"), directory.list()!!.toList())
            assertFalse(contents(store).contains("a".repeat(30)))
        }
    }
    @Test fun capIsEnforcedWhileTheCurrentHourStillFitsByItself() {
        val directory = temporary.newFolder()
        val value = line(time, "a".repeat(30))
        val cap = (value.toByteArray().size + 1L) * 3
        LogSegmentStore(directory, { time }, cap).use { store ->
            store.append(line(time - 3_600_000, "a".repeat(30)))
            repeat(3) { store.append(value) }
            store.flush()
            assertTrue(directory.listFiles()!!.sumOf { it.length() } <= cap)
            assertFalse(File(directory, "relay-20261023-04.log").exists())
        }
    }
    @Test fun snapshotsKeepTheirOriginalLengthWhileRecordingContinues() {
        LogSegmentStore(temporary.newFolder(), { time }).use { store ->
            store.append(line(time, "first"))
            store.snapshot().use { snapshot ->
                store.append(line(time + 1, "second")); store.flush()
                val text = snapshot.streams.single().bufferedReader().readText()
                assertTrue(text.contains("first")); assertFalse(text.contains("second"))
            }
        }
    }
    @Test fun snapshotsSkipEarlierHoursAndKeepTheCutoffHourAndUnparseableNamesInOrder() {
        val directory = temporary.newFolder()
        File(directory, "relay-20261023-03.log").writeText("old\n")
        File(directory, "relay-20261023-04.log").writeText("boundary\n")
        File(directory, "relay-20261023-04-2.log").writeText("boundary two\n")
        File(directory, "relay-20261023-05.log").writeText("recent\n")
        File(directory, "relay-20261023-99.log").writeText("unparseable\n")
        LogSegmentStore(directory, { time }).use { store ->
            store.snapshot(time - 600_000).use { snapshot ->
                assertTrue(snapshot.skippedEarlier)
                assertEquals(listOf("boundary\n", "boundary two\n", "recent\n", "unparseable\n"),
                    snapshot.streams.map { it.bufferedReader().readText() })
            }
            store.snapshot(time - 300_000).use { snapshot ->
                assertTrue(snapshot.skippedEarlier)
                assertEquals(listOf("recent\n", "unparseable\n"), snapshot.streams.map { it.bufferedReader().readText() })
            }
            store.snapshot().use { snapshot ->
                assertFalse(snapshot.skippedEarlier)
                assertEquals(listOf("old\n", "boundary\n", "boundary two\n", "recent\n", "unparseable\n"),
                    snapshot.streams.map { it.bufferedReader().readText() })
            }
        }
    }
    @Test fun resumeFindsTheLastTimestampEvenAfterUnstampedLinesAndNumericSuffixes() {
        val directory = temporary.newFolder()
        File(directory, "relay-20261023-05-2.log").writeText(line(time, "two") + "\n")
        File(directory, "relay-20261023-05-10.log").writeText(line(time + 1, "ten") + "\nunstamped\n")
        LogSegmentStore(directory, { time }).use { store ->
            assertEquals(time + 1, store.lastTimestamp)
            assertTrue(contents(store).indexOf("two") < contents(store).indexOf("ten"))
        }
    }
    @Test fun zipContainsOnlyTheRangeInSegmentOrderVerbatim() = runBlocking {
        val cache = temporary.newFolder()
        LogSegmentStore(temporary.newFolder(), { time }).use { store ->
            val included = listOf(line(time - 600_000, "password=[secret#0a1b2c]"), "included continuation [secret#0a1b2c]",
                line(time - 100, "newer"), line(time, "last"))
            store.append(line(time - 600_001, "too old")); store.append("old continuation")
            included.forEach(store::append)
            store.append(line(time + 1, "future")); store.append("future continuation")
            val exporter = LogExporter(cache, store, { "App: Test" }, { time }, { ZoneId.of("UTC") })
            val file = checkNotNull(exporter.export(LogRange.LAST_10_MINUTES))
            ZipFile(file).use { zip ->
                assertEquals(setOf("relay.log", "info.txt"), zip.entries().asSequence().map { it.name }.toSet())
                val bytes = zip.getInputStream(zip.getEntry("relay.log")).readBytes()
                assertArrayEquals(included.joinToString("\n", postfix = "\n").toByteArray(Charsets.UTF_8), bytes)
                val log = String(bytes, Charsets.UTF_8)
                assertFalse(log.contains("too old")); assertFalse(log.contains("old continuation")); assertFalse(log.contains("future"))
                val info = zip.getInputStream(zip.getEntry("info.txt")).bufferedReader().readText()
                assertTrue(info.contains("Private values (passwords, keys, phone numbers, names, messages, media and navigation text, precise location) were replaced by tokens when the logs were recorded; the same value gives the same token on this phone."))
                assertTrue(info.contains("LAST_10_MINUTES"))
            }
        }
    }
    @Test fun emptyRangeProducesNoZipAndDeletesOldExports() = runBlocking {
        val cache = temporary.newFolder()
        File(cache, "old.zip").writeText("old")
        LogSegmentStore(temporary.newFolder(), { time }).use { store ->
            store.append(line(time - 86_400_001, "old"))
            val exporter = LogExporter(cache, store, { "App: Test" }, { time })
            assertNull(exporter.export(LogRange.LAST_24_HOURS))
            assertTrue(cache.list()!!.isEmpty())
        }
    }
    @Test fun missingHistoryIsExplainedInInfo() = runBlocking {
        LogSegmentStore(temporary.newFolder(), { time }).use { store ->
            store.append(line(time, "only line"))
            val file = checkNotNull(LogExporter(temporary.newFolder(), store, { "App: Test" }, { time })
                .export(LogRange.LAST_30_MINUTES))
            ZipFile(file).use { zip ->
                assertTrue(zip.getInputStream(zip.getEntry("info.txt")).bufferedReader().readText().contains("64 MiB"))
            }
        }
    }
    @Test fun quietPeriodsAfterTheCutoffAreNotReportedAsMissingHistory() = runBlocking {
        LogSegmentStore(temporary.newFolder(), { time }).use { store ->
            store.append(line(time - 3_600_000, "older than the range")); store.append(line(time - 60_000, "recent"))
            val file = checkNotNull(LogExporter(temporary.newFolder(), store, { "App: Test" }, { time })
                .export(LogRange.LAST_30_MINUTES))
            ZipFile(file).use { zip ->
                assertFalse(zip.getInputStream(zip.getEntry("info.txt")).bufferedReader().readText().contains("64 MiB"))
            }
        }
    }
    @Test fun segmentsWhollyBeforeTheCutoffAreNeverOpenedOrReportedAsMissingHistory() = runBlocking {
        val directory = temporary.newFolder()
        LogSegmentStore(directory, { time }).use { store ->
            store.append(line(time - 2 * 3_600_000, "old")); store.append(line(time, "recent")); store.flush()
            val old = File(directory, "relay-20261023-03.log")
            old.setReadable(false)
            try {
                assumeFalse(old.canRead())
                val file = checkNotNull(LogExporter(temporary.newFolder(), store, { "App: Test" }, { time })
                    .export(LogRange.LAST_10_MINUTES))
                ZipFile(file).use { zip ->
                    assertEquals(line(time, "recent") + "\n", zip.getInputStream(zip.getEntry("relay.log")).bufferedReader().readText())
                    assertFalse(zip.getInputStream(zip.getEntry("info.txt")).bufferedReader().readText().contains("64 MiB"))
                }
            } finally { old.setReadable(true) }
        }
    }
    @Test fun cancellationDuringStreamingDeletesThePartialZip() = runBlocking {
        val cache = temporary.newFolder()
        LogSegmentStore(temporary.newFolder(), { time }).use { store ->
            repeat(200) { store.append(line(time, "message $it")) }
            val worker = launch {
                val parent = coroutineContext
                try {
                    LogExporter(cache, store, { "App: Test" }, { time }, flush = { store.flush(); parent.cancel() })
                        .export(LogRange.LAST_10_MINUTES)
                    fail("Export should have been cancelled")
                } catch (_: CancellationException) { }
            }
            worker.join()
            assertTrue(cache.list()!!.isEmpty())
        }
    }
}
