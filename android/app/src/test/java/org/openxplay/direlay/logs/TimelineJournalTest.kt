// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-11-01
package org.openxplay.direlay.logs

import kotlinx.coroutines.runBlocking
import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder
import java.io.File
import java.time.Instant
import java.util.concurrent.CountDownLatch
import java.util.concurrent.Executors
import java.util.concurrent.TimeUnit
import java.util.zip.ZipFile

class TimelineJournalTest {
    @get:Rule val temporary = TemporaryFolder()
    private val time = Instant.parse("2026-11-01T05:05:00Z").toEpochMilli()
    private fun contents(journal: TimelineJournal): String = journal.snapshot(Long.MIN_VALUE).use { snapshot ->
        snapshot.streams.joinToString("") { it.bufferedReader().readText() }
    }
    private fun entry(zip: ZipFile, name: String) = zip.getInputStream(zip.getEntry(name)).bufferedReader().use { it.readText() }

    @Test fun journalRotatesByHourAndEnforcesItsOwnByteCap() {
        val directory = temporary.newFolder()
        TimelineJournal({ directory }, { time }, cap = 160).use { journal ->
            journal.append("OXP-Timeline", "old", time - 3_600_000)
            journal.append("OXP-Timeline", "new", time)
            assertEquals(2, directory.listFiles()!!.size)
            repeat(5) { journal.append("OXP-Timeline", "latest $it", time + it) }
            assertTrue(directory.listFiles()!!.sumOf { it.length() } <= 160)
            assertTrue(directory.listFiles()!!.any { it.name.endsWith("-2.log") || it.name.endsWith("-3.log") })
            assertFalse(contents(journal).contains("old"))
            assertTrue(contents(journal).contains("latest 4"))
            val before = contents(journal)
            journal.append("OXP-Timeline", "x".repeat(200), time)
            assertEquals(before, contents(journal))
        }
    }
    @Test fun journalUsesTheSameHourEndRetentionAndPrunesWhileQuiet() {
        var now = time
        val directory = temporary.newFolder()
        TimelineJournal({ directory }, { now }).use { journal ->
            journal.append("OXP-Timeline", "boundary", time - 24 * 3_600_000L)
            journal.append("OXP-Timeline", "recent", time)
            assertTrue(contents(journal).contains("boundary"))
            now += 3_600_001
            journal.maintenance()
            assertFalse(contents(journal).contains("boundary"))
            assertTrue(contents(journal).contains("recent"))
        }
    }
    @Test fun restartReadsFlushedLinesEvenBeforeThePreviousWriterCloses() {
        val directory = temporary.newFolder()
        TimelineJournal({ directory }, { time }).use { before ->
            before.append("OXP-Timeline", "before crash", time)
            TimelineJournal({ directory }, { time }).use { after ->
                assertTrue(contents(after).contains("before crash"))
                after.append("OXP-Timeline", "after restart", time + 1)
                assertTrue(contents(after).contains("before crash"))
                assertTrue(contents(after).contains("after restart"))
            }
        }
    }
    @Test fun connectionLogWritesIdenticalRedactedMessagesAndNeverStoresTheSecret() {
        val directory = temporary.newFolder()
        val journal = TimelineJournal({ directory }, { time })
        val lines = mutableListOf<Pair<String, String>>()
        val log = ConnectionLog(clock = { 0 }, write = { tag, message -> lines += tag to message },
            journal = journal, wallClock = { time })
        try {
            log.event("relay", "process_start", "password=never-store-this-secret")
            log.diagnostic("passphrase=another-private-value")
            log.awaitIdle()
            val secrets = LogSecrets().apply { remember(listOf("configured-opaque-value")) }
            log.filterWith(LogRedactor(secrets)::redact)
            log.event("relay", "configured", "configured-opaque-value")
            log.awaitIdle()
            val text = directory.listFiles()!!.joinToString("") { it.readText() }
            assertFalse(text.contains("never-store-this-secret"))
            assertFalse(text.contains("another-private-value"))
            assertFalse(text.contains("configured-opaque-value"))
            assertTrue(text.contains("[secret#"))
            assertEquals(lines.map { it.second }, text.lineSequence().filter { it.isNotEmpty() }.map(LogLine::message).toList())
        } finally { log.close() }
    }
    @Test fun blockedJournalNeverBlocksTheConnectionCallerAndLogcatFailureKeepsTheJournal() {
        val entered = CountDownLatch(1)
        val release = CountDownLatch(1)
        val directory = temporary.newFolder()
        val journal = TimelineJournal({ entered.countDown(); release.await(); directory }, { time })
        val log = ConnectionLog(clock = { 0 }, write = { _, _ -> error("logcat unavailable") }, journal = journal, wallClock = { time })
        val producer = Executors.newSingleThreadExecutor()
        try {
            producer.submit { log.event("relay", "process_start") }.get(5, TimeUnit.SECONDS)
            assertTrue(entered.await(5, TimeUnit.SECONDS))
            producer.submit { log.event("relay", "app_ready") }.get(5, TimeUnit.SECONDS)
            release.countDown(); log.awaitIdle()
            assertTrue(contents(journal).contains("process_start"))
            assertTrue(contents(journal).contains("app_ready"))
        } finally { release.countDown(); log.close(); producer.shutdownNow() }
    }
    @Test fun errorOnlyLogcatStillExportsLifecycleConnectionsAndEveryOutput() = runBlocking {
        var elapsed = 0L
        val journal = TimelineJournal({ temporary.newFolder() }, { time })
        LogSegmentStore(temporary.newFolder(), { time }).use { store ->
            val probe = LogcatProbe({ elapsed }, { "test-nonce" })
            val recorder = LogRecorder(store, LogRedactor(), clock = { elapsed }, probe = probe)
            val log = ConnectionLog(clock = { elapsed }, write = { _, _ -> }, journal = journal, wallClock = { time })
            try {
                probe.start { priority, marker ->
                    if (priority == 'E') recorder.capture("2026-11-01 05:05:00.000 +0000 1 2 E ${LogcatProbe.TAG}: $marker")
                    20
                }
                log.event("relay", "process_start")
                log.transition("network", "wifi", "wifi_connected")
                log.phone("attempt_started")
                log.phone("session_active")
                log.event("browser", "client_opened")
                log.carlink("link_start")
                log.castEvent("output_started", connection = 1)
                log.linkEnded("phone", LinkEnd("wireless_run", "session_active", "peer", "peer_eof"))
                log.event("relay", "app_hidden")
                elapsed = 5_000
                val exporter = LogExporter(temporary.newFolder(), store, { "App: Test" }, { time },
                    flush = { log.awaitIdle(); recorder.flush() }, journal = journal, probe = probe::result)
                val archive = checkNotNull(exporter.export(LogRange.LAST_10_MINUTES))
                ZipFile(archive).use { zip ->
                    assertEquals(setOf("relay.log", "timeline.log", "info.txt"), zip.entries().asSequence().map { it.name }.toSet())
                    val timeline = entry(zip, "timeline.log")
                    for (event in listOf("relay process_start", "network wifi_connected", "phone attempt_started", "phone session_active",
                        "browser client_opened", "carlink link_start", "cast output_started", "phone link_ended", "phone interrupted", "phone context", "relay app_hidden"))
                        assertTrue(event, timeline.contains(event))
                    assertFalse(entry(zip, "relay.log").contains("OXP-Timeline"))
                    val info = entry(zip, "info.txt")
                    assertTrue(info.contains("Capture completeness"))
                    assertTrue(info.contains("Logcat capture: 1 lines"))
                    assertTrue(info.contains("Timeline journal: ${timeline.lines().count { it.isNotEmpty() }} lines; first 2026-11-01T05:05:00Z; last 2026-11-01T05:05:00Z"))
                    assertTrue(info.contains("logcat returned E only"))
                    assertTrue(info.contains("INFO, WARN"))
                    assertTrue(info.contains("Capture is incomplete"))
                    assertTrue(exporter.captureIncomplete)
                }
            } finally { log.close(); recorder.close() }
        }
    }
    @Test fun journalAloneMakesAnExportAndRespectsTheSelectedRange() = runBlocking {
        TimelineJournal({ temporary.newFolder() }, { time }).use { journal ->
            journal.append("OXP-Timeline", "too old", time - 600_001)
            journal.append("OXP-Timeline", "boundary", time - 600_000)
            journal.append("OXP-Timeline", "recent", time)
            journal.append("OXP-Timeline", "future", time + 1)
            LogSegmentStore(temporary.newFolder(), { time }).use { store ->
                val archive = checkNotNull(LogExporter(temporary.newFolder(), store, { "App: Test" }, { time }, journal = journal)
                    .export(LogRange.LAST_10_MINUTES))
                ZipFile(archive).use { zip ->
                    assertEquals("", entry(zip, "relay.log"))
                    val timeline = entry(zip, "timeline.log")
                    assertTrue(timeline.contains("boundary")); assertTrue(timeline.contains("recent"))
                    assertFalse(timeline.contains("too old")); assertFalse(timeline.contains("future"))
                    assertTrue(entry(zip, "info.txt").contains("Timeline journal: 2 lines"))
                }
            }
        }
    }
    @Test fun captureFailuresAreReportedEvenWhenAllPrioritiesReturned() = runBlocking {
        TimelineJournal({ temporary.newFolder() }, { time }).use { journal ->
            journal.append("OXP-Timeline", "before cutoff", time - 600_001)
            journal.append("OXP-Timeline", "retained", time)
            LogSegmentStore(temporary.newFolder(), { time }).use { store ->
                store.append("2026-11-01 04:54:59.999 +0000 1 2 I Test: before cutoff")
                store.append("2026-11-01 05:05:00.000 +0000 1 2 E Test: retained")
                val result = LogcatProbe.Result(LogcatProbe.State.COMPLETE, setOf('V', 'D', 'I', 'W', 'E'), emptyMap())
                val exporter = LogExporter(temporary.newFolder(), store, { "App: Test" }, { time }, journal = journal,
                    probe = { result }, recorderFailures = { 2 })
                val archive = checkNotNull(exporter.export(LogRange.LAST_10_MINUTES))
                ZipFile(archive).use { zip ->
                    assertTrue(entry(zip, "info.txt").contains("Recorder redaction/storage failures or skipped writes: 2"))
                    assertTrue(entry(zip, "info.txt").contains("Capture is incomplete"))
                }
                assertTrue(exporter.captureIncomplete)
            }
        }
    }
    @Test fun failedJournalWritesAreIsolatedAndReportedAlongWithWorkerDrops() = runBlocking {
        val file = temporary.newFile()
        TimelineJournal({ file }, { time }).use { journal ->
            journal.append("OXP-Timeline", "unwritable", time)
            assertEquals(1L, journal.failedWrites)
            LogSegmentStore(temporary.newFolder(), { time }).use { store ->
                store.append("2026-11-01 05:05:00.000 +0000 1 2 E Test: retained")
                val exporter = LogExporter(temporary.newFolder(), store, { "App: Test" }, { time }, journal = journal, timelineDropped = { 3 })
                val archive = checkNotNull(exporter.export(LogRange.LAST_10_MINUTES))
                ZipFile(archive).use { zip ->
                    assertTrue(entry(zip, "info.txt").contains("Timeline worker dropped: 3; journal failed writes: 1"))
                    assertTrue(entry(zip, "info.txt").contains("Timeline journal unavailable during export"))
                    assertTrue(entry(zip, "relay.log").contains("retained"))
                }
                assertTrue(exporter.captureIncomplete)
            }
        }
    }
}
