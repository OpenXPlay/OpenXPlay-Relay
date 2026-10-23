// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-18

package org.openxplay.direlay.carlinkdebug

import android.app.Application
import java.nio.file.Files
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.openxplay.direlay.clink.*
import org.openxplay.stream.clink.trace.*

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [33], manifest = Config.NONE, application = Application::class)
class CarlinkTraceRecorderTest {
    @Test fun callbacksDoNotWaitForWriterAndNumericFieldsStayNumeric() {
        val directory = Files.createTempDirectory("carlink-async").toFile()
        val entered = java.util.concurrent.CountDownLatch(1)
        val release = java.util.concurrent.CountDownLatch(1)
        val recorder = CarlinkTraceRecorder(directory, { entered.countDown(); release.await() })
        try {
            val frame = ClinkTraceEvent.Frame(12, 3, "RX", 123, "control", 1, 2, 0, 9, 8,
                mapOf(1 to ClinkTraceField.Number(42uL), 2 to ClinkTraceField.Bytes(100, "preview")))
            recorder.record(frame)
            assertTrue(entered.await(2, java.util.concurrent.TimeUnit.SECONDS))
            val callback = java.util.concurrent.Executors.newSingleThreadExecutor()
            try { callback.submit { recorder.record(frame) }.get(1, java.util.concurrent.TimeUnit.SECONDS) }
            finally { callback.shutdownNow() }
            release.countDown(); recorder.awaitIdle()
            val fields = JSONObject(recorder.snapshot().first().json).getJSONObject("fields")
            assertTrue(fields.get("1") is Number)
            assertEquals(42, fields.getInt("1")); assertEquals(100, fields.getJSONObject("2").getInt("length"))
        } finally { release.countDown(); recorder.close(); directory.deleteRecursively() }
    }

    @Test fun inputTraceKeepsRawCoordinatesAndCurrentPictureSize() {
        val event = ClinkTraceEvent.Input(1, 2, "touch", listOf(.5 to .25),
            listOf(640L to 180L), 1156 to 764, 1532 to 1013)
        val json = JSONObject(CarlinkTraceRecorder.encode(event))
        assertEquals(.5, json.getJSONArray("coordinates").getJSONArray(0).getDouble(0), 0.0)
        assertEquals(640L, json.getJSONArray("rawCoordinates").getJSONArray(0).getLong(0))
        assertEquals(180L, json.getJSONArray("rawCoordinates").getJSONArray(0).getLong(1))
        assertEquals(1156, json.getJSONArray("streamSize").getInt(0))
        assertEquals(764, json.getJSONArray("streamSize").getInt(1))
        assertEquals(1532, json.getJSONArray("viewSize").getInt(0))
        assertEquals(1013, json.getJSONArray("viewSize").getInt(1))
        val legacy = JSONObject(CarlinkTraceRecorder.encode(ClinkTraceEvent.Input(1, 2, "touch", emptyList())))
        assertEquals(0, legacy.getJSONArray("rawCoordinates").length())
        assertTrue(legacy.isNull("streamSize"))
        assertTrue(legacy.isNull("viewSize"))
    }

    @Test fun ringRotationJsonAndClear() {
        val directory = Files.createTempDirectory("carlink-trace").toFile()
        val mirror = mutableListOf<String>()
        val recorder = CarlinkTraceRecorder(directory, { mirror += it }, maxBytes = 300, keepFiles = 8)
        try {
            repeat(5100) { i -> recorder.record(ClinkTraceEvent.Link(i.toLong(), 7, "stage", mapOf("index" to i.toString())))
                if (i % 100 == 0) recorder.awaitIdle()
            }
            recorder.awaitIdle()
            assertEquals(5000, recorder.snapshot().size)
            assertEquals(100L, JSONObject(recorder.snapshot().first().json).getLong("atNanos"))
            assertEquals(8, directory.listFiles()!!.size)
            assertEquals(5100, mirror.size)
            directory.listFiles()!!.forEach { file ->
                assertTrue(file.length() <= 300)
                file.readLines().forEach { line ->
                    val json = JSONObject(line)
                    assertEquals("Link", json.getString("type")); assertEquals(7, json.getInt("session"))
                    assertTrue(json.has("atNanos")); assertTrue(json.has("fields"))
                }
            }
            assertTrue(recorder.currentFile!!.exists())
            recorder.clear(); recorder.awaitIdle()
            assertTrue(recorder.snapshot().isEmpty()); assertEquals(0, directory.listFiles()!!.size)
        } finally { recorder.close(); directory.deleteRecursively() }
    }
    @Test fun vehicleIdentificationIsMirrored() {
        val directory = Files.createTempDirectory("carlink-identification").toFile()
        val mirrored = mutableListOf<String>()
        val recorder = CarlinkTraceRecorder(directory, { mirrored += it })
        val delegated = mutableListOf<ClinkDiagnostic>()
        try {
            val diagnostics = TracingClinkDiagnostics(ClinkDiagnostics { delegated += it }, recorder)
            diagnostics.traceLink("ble_advertisement", mapOf("major" to "1", "minor" to "9", "serial" to "7",
                "carId" to "001122", "modelId" to "3344", "vendorData" to "5566", "name" to "Car"))
            diagnostics.traceLink("ble_reply_name", mapOf("name" to "Head Unit"))
            recorder.awaitIdle()
            assertTrue(delegated.isEmpty()); assertEquals(2, mirrored.size)
            assertTrue(mirrored.first().contains("ble_advertisement")); assertTrue(mirrored.last().contains("Head Unit"))
            val content = recorder.currentFile!!.readText()
            assertTrue(content.contains("ble_advertisement")); assertTrue(content.contains("Head Unit"))
            assertFalse(content.contains("psk"))
            TracingClinkDiagnostics(ClinkDiagnostics { error("trace must bypass delegate") }, ClinkTrace.NONE)
                .traceLink("ble_reply_name", mapOf("name" to "Disabled"))
        } finally { recorder.close(); directory.deleteRecursively() }
    }
    @Test fun redactionAndDisabledPort() {
        val directory = Files.createTempDirectory("carlink-redaction").toFile()
        val recorder = CarlinkTraceRecorder(directory)
        try {
            val diagnostic = TracingClinkDiagnostics(ClinkDiagnostics.NONE, recorder)
            diagnostic.emit(ClinkDiagnostic.Handoff("Car WiFi", "wifi-secret".length))
            diagnostic.emit(ClinkStage.AUTH_PIN)
            for (service in listOf("auth", "cert")) recorder.record(ClinkTraceEvent.Frame(1, 2, "RX", 1, service,
                1, 1, 1, 1, 42, mapOf(1 to ClinkTraceField.Bytes(42, "012345 private-key accessory-identity cert-body"))))
            assertSame(ClinkTrace.NONE, CarlinkTraceRecorder.select(false) { error("disabled recorder must not be initialized") })
            CarlinkTraceRecorder.select(false) { error("disabled recorder must not be initialized") }.record(ClinkTraceEvent.Link(0, 0, "disabled"))
            recorder.awaitIdle()
            val content = recorder.currentFile!!.readText()
            for (secret in listOf("wifi-secret", "012345", "private-key", "accessory-identity", "cert-body", "disabled")) assertFalse(content.contains(secret))
            assertTrue(content.contains("Car WiFi")); assertTrue(content.contains("passphraseLength"))
            assertTrue(content.contains("bodySize"))
        } finally { recorder.close(); directory.deleteRecursively() }
    }
    @Test fun mediaCaptureStopsAtDeadlineWithoutPackets() {
        val directory = Files.createTempDirectory("carlink-deadline").toFile()
        val capture = CarlinkMediaCapture(directory, durationNs = 20_000_000)
        try {
            capture.start()
            val deadline = System.nanoTime() + 2_000_000_000
            while (capture.capturing && System.nanoTime() < deadline) Thread.sleep(5)
            assertFalse(capture.capturing)
            capture.close()
            assertEquals(0L, directory.listFiles()!!.single().length())
        } finally { capture.close(); directory.deleteRecursively() }
    }

    @Test fun mediaCaptureStripsRtpAndStopsAtLimit() {
        val directory = Files.createTempDirectory("carlink-media").toFile()
        val capture = CarlinkMediaCapture(directory, maxBytes = 188)
        try {
            capture.start()
            val packet = ByteArray(200) { if (it < 12) 0 else 0x47 }
            capture.rtp(packet, 0, packet.size)
            val deadline = System.nanoTime() + 2_000_000_000
            while (capture.capturing && System.nanoTime() < deadline) Thread.sleep(5)
            assertFalse(capture.capturing); capture.close()
            assertEquals(188, directory.listFiles()!!.single().length().toInt())
            assertTrue(directory.listFiles()!!.single().readBytes().all { it == 0x47.toByte() })
        } finally { capture.close(); directory.deleteRecursively() }
    }
    @Test fun experimentalRawRecordsStayInJsonlWhileExperimentAndAuthLinesReachTrailAndTrace() {
        val directory = Files.createTempDirectory("carlink-experiments").toFile()
        val mirrored = mutableListOf<String>()
        val delegated = mutableListOf<String>()
        val recorder = CarlinkTraceRecorder(directory, { mirrored += it })
        try {
            val diagnostics = TracingClinkDiagnostics(ClinkDiagnostics { delegated += it.line() }, recorder)
            diagnostics.emit(ClinkStage.LINK_START)
            diagnostics.emit(ClinkDiagnostic.Experiments(listOf("keep_scanning", "record_advertisements")))
            diagnostics.traceLink("ble_raw", mapOf("address" to "AA:BB:CC:DD:EE:FF", "raw" to "010203"))
            diagnostics.traceLink("ble_raw_dropped", mapOf("count" to "2"))
            diagnostics.traceLink("ble_raw_skipped", mapOf("reason" to "scan_budget", "count" to "3"))
            diagnostics.emit(ClinkDiagnostic.Auth("auth repin reason=peer_key_changed"))
            recorder.awaitIdle()
            assertEquals(listOf("link_start", "experiments keep_scanning,record_advertisements", "auth repin reason=peer_key_changed"), delegated)
            assertEquals(3, mirrored.size)
            assertFalse(mirrored.any { it.contains("AA:BB:CC:DD:EE:FF") || it.contains("ble_raw") })
            val events = recorder.currentFile!!.readLines().map { JSONObject(it) }
            assertEquals(listOf("link_start", "experiments", "ble_raw", "ble_raw_dropped", "ble_raw_skipped", "auth"), events.map { it.getString("stage") })
            assertEquals("010203", events[2].getJSONObject("fields").getString("raw"))
            assertEquals("scan_budget", events[4].getJSONObject("fields").getString("reason"))
            assertEquals("3", events[4].getJSONObject("fields").getString("count"))
            assertEquals("auth repin reason=peer_key_changed", events.last().getJSONObject("fields").getString("detail"))
        } finally { recorder.close(); directory.deleteRecursively() }
    }

}
