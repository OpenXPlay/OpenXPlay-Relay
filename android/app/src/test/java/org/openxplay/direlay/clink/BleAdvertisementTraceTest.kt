// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-23
package org.openxplay.direlay.clink

import android.app.Application
import org.json.JSONArray
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.openxplay.stream.clink.ble.BleCodec
import java.util.concurrent.AbstractExecutorService
import java.util.concurrent.TimeUnit

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [33], manifest = Config.NONE, application = Application::class)
class BleAdvertisementTraceTest {
    private class History : BleAdvertisementTrace.Store {
        val serials = mutableMapOf<String, Int>()
        var writes = 0
        override fun readSerial(carId: String) = serials[carId]
        override fun writeSerial(carId: String, serial: Int) { writes++; serials[carId] = serial }
    }
    private class Diagnostics : ClinkDiagnostics {
        val links = mutableListOf<Pair<String, Map<String, String>>>()
        val trail = mutableListOf<String>()
        override fun emit(event: ClinkDiagnostic) { trail.add(event.line()) }
        override fun traceLink(stage: String, fields: Map<String, String>) { links.add(stage to fields) }
    }
    private class QueuedExecutor : AbstractExecutorService() {
        private val tasks = java.util.ArrayDeque<Runnable>()
        private var closed = false
        override fun execute(command: Runnable) { check(!closed); tasks.addLast(command) }
        override fun shutdown() { closed = true }
        override fun shutdownNow(): MutableList<Runnable> = tasks.toMutableList().also { tasks.clear(); shutdown() }
        override fun isShutdown() = closed
        override fun isTerminated() = closed && tasks.isEmpty()
        override fun awaitTermination(timeout: Long, unit: TimeUnit) = isTerminated()
        fun runNext() { tasks.removeFirst().run() }
        fun runAll() { while (tasks.isNotEmpty()) runNext() }
    }
    private fun raw() = BleRawAdvertisement("AA:BB:CC:DD:EE:FF", 1, -71, true, false, -8, "Nearby device",
        listOf(BleCodec.ADVERTISEMENT_SERVICE), mapOf(BleCodec.NAME_SERVICE to byteArrayOf(0, 0x7f, 0xff.toByte())),
        mapOf(76 to byteArrayOf(0xab.toByte(), 0x01)), byteArrayOf(0, 0x80.toByte(), 0xff.toByte()))

    @Test fun hexMatchesFormatterForEveryByteAndEmptyPayloads() {
        val bytes = ByteArray(256) { it.toByte() }
        assertEquals(bytes.joinToString("") { "%02x".format(it.toInt() and 255) }, BleRawAdvertisement.hex(bytes))
        assertEquals("", BleRawAdvertisement.hex(byteArrayOf()))
    }

    @Test fun recordKeepsTheGoldenTraceFields() {
        val diagnostics = Diagnostics()
        val identity = ByteArray(15) { it.toByte() }.apply { this[0] = 1; this[1] = 9; this[2] = 7 }
        BleAdvertisementTrace(diagnostics, History()).scan().record(raw().copy(serviceData = mapOf(
            BleCodec.NAME_SERVICE to "Car".toByteArray(), BleCodec.IDENTITY_SERVICE to identity)), 0)
        assertEquals(listOf("ble_raw" to mapOf(
            "address" to "AA:BB:CC:DD:EE:FF", "addressType" to "1", "rssi" to "-71",
            "connectable" to "true", "legacy" to "false", "txPower" to "-8", "localName" to "Nearby device",
            "serviceUuids" to "[\"0000fcfb-0000-1000-8000-00805f9b34fb\"]",
            "serviceData" to "{\"00000001-0000-1000-8000-00805f9b34fb\":\"010907030405060708090a0b0c0d0e\",\"00000002-0000-1000-8000-00805f9b34fb\":\"436172\"}",
            "manufacturerData" to "{\"76\":\"ab01\"}", "raw" to "0080ff",
            "carId" to "030405060708", "serial" to "7", "serialSame" to "false")), diagnostics.links)
    }

    @Test fun rawFormattingKeepsAllAvailableFieldsAndHexPayloads() {
        val fields = raw().fields()
        assertEquals("AA:BB:CC:DD:EE:FF", fields["address"])
        assertEquals("1", fields["addressType"]); assertEquals("-71", fields["rssi"])
        assertEquals("true", fields["connectable"]); assertEquals("false", fields["legacy"])
        assertEquals("-8", fields["txPower"]); assertEquals("Nearby device", fields["localName"])
        assertEquals(BleCodec.ADVERTISEMENT_SERVICE, JSONArray(fields.getValue("serviceUuids")).getString(0))
        assertEquals("007fff", JSONObject(fields.getValue("serviceData")).getString(BleCodec.NAME_SERVICE))
        assertEquals("ab01", JSONObject(fields.getValue("manufacturerData")).getString("76"))
        assertEquals("0080ff", fields["raw"])
        assertFalse(raw().copy(addressType = null).fields().containsKey("addressType"))
    }

    @Test fun throttleSeparatesAddressAndPayloadAndUsesRollingLimits() {
        val throttle = BleRawThrottle()
        assertTrue(throttle.accept("a", "payload", 0))
        assertFalse(throttle.accept("a", "payload", 1999))
        assertTrue(throttle.accept("a", "payload", 2000))
        assertTrue(throttle.accept("b", "payload", 2000))
        assertTrue(throttle.accept("a", "changed", 2000))
        assertEquals(1L, throttle.dropped)
        val capped = BleRawThrottle()
        repeat(50) { assertTrue(capped.accept("$it", "payload", 100)) }
        assertFalse(capped.accept("51", "payload", 1099))
        assertTrue(capped.accept("51", "payload", 1100))
        assertFalse(capped.accept("51", "payload", 1101))
        assertEquals(2L, capped.dropped)
    }

    @Test fun rawCarRecordsCompareOnlyWithTheLastAuthenticatedSerialAndReportDropsOnce() {
        val history = History()
        val diagnostics = Diagnostics()
        val trace = BleAdvertisementTrace(diagnostics, history)
        val identity = ByteArray(15) { it.toByte() }.apply { this[0] = 1; this[1] = 9; this[2] = 7 }
        val advertisement = BleCodec.advertisement(identity)
        val raw = raw().copy(serviceData = mapOf(BleCodec.IDENTITY_SERVICE to identity))
        val scan = trace.scan()
        scan.record(raw, 0); scan.record(raw, 1)
        assertEquals("ble_raw", diagnostics.links.single().first)
        assertEquals(advertisement.carId, diagnostics.links.single().second["carId"])
        assertEquals("7", diagnostics.links.single().second["serial"])
        assertEquals("false", diagnostics.links.single().second["serialSame"])
        assertTrue(history.serials.isEmpty())
        trace.selected(advertisement)
        assertEquals(0, history.writes)
        trace.connected(); trace.connected()
        assertEquals(1, history.writes)
        assertEquals("true", BleAdvertisementTrace(diagnostics, history).serialFields(advertisement)["serialSame"])
        assertEquals("false", trace.serialFields(advertisement.copy(serial = 8))["serialSame"])
        assertEquals("false", trace.serialFields(advertisement.copy(carId = "another"))["serialSame"])
        scan.finish(); scan.finish(); scan.record(raw, 5000)
        assertEquals("ble_raw_dropped" to mapOf("count" to "1"), diagnostics.links.last())
        assertEquals(2, diagnostics.links.size)
        assertTrue(diagnostics.trail.isEmpty())
    }

    @Test fun malformedCarRecordsStayRawAndOnlyAuthSessionDiagnosticsAreForwarded() {
        val diagnostics = Diagnostics()
        val trace = BleAdvertisementTrace(diagnostics, History())
        val scan = trace.scan()
        scan.record(raw().copy(serviceData = mapOf(BleCodec.IDENTITY_SERVICE to byteArrayOf(1))), 0)
        assertFalse(diagnostics.links.single().second.containsKey("carId"))
        trace.sessionDiagnostic("auth repin reason=peer_key_changed")
        trace.sessionDiagnostic("audio queue reset")
        assertEquals(listOf("auth repin reason=peer_key_changed"), diagnostics.trail)
    }

    @Test fun captureDefersExtractionAndOrdersRecordsFailuresAndFinish() {
        val diagnostics = Diagnostics()
        val executor = QueuedExecutor()
        val capture = BleRawCapture(BleAdvertisementTrace(diagnostics, History()).scan(), executor) { 0L }
        val reads = mutableListOf<String>()
        capture.record { reads.add("first"); raw() }
        capture.failed(4)
        capture.record { reads.add("second"); raw().copy(address = "second") }
        capture.record { reads.add("bad"); throw IllegalArgumentException("bad scan") }
        capture.finish(); capture.finish()
        capture.record { reads.add("late"); raw() }; capture.failed(5)
        assertTrue(reads.isEmpty()); assertTrue(diagnostics.links.isEmpty())
        assertTrue(executor.isShutdown())
        executor.runAll()
        assertEquals(listOf("first", "second", "bad"), reads)
        assertEquals(listOf("ble_raw", "ble_raw_failed", "ble_raw", "ble_raw_failed", "ble_raw_dropped"),
            diagnostics.links.map { it.first })
        assertEquals("AA:BB:CC:DD:EE:FF", diagnostics.links[0].second["address"])
        assertEquals(mapOf("errorCode" to "4"), diagnostics.links[1].second)
        assertEquals("second", diagnostics.links[2].second["address"])
        assertEquals(mapOf("errorCode" to "-1"), diagnostics.links[3].second)
        assertEquals(mapOf("count" to "0"), diagnostics.links.last().second)
        capture.record { reads.add("finished"); raw() }; capture.failed(6); capture.finish()
        executor.runAll()
        assertEquals(3, reads.size); assertEquals(5, diagnostics.links.size)
        assertTrue(executor.isTerminated())
    }

    @Test fun captureBoundsPendingResultsAndCountsOverflowWithThrottleDrops() {
        val diagnostics = Diagnostics()
        val executor = QueuedExecutor()
        val capture = BleRawCapture(BleAdvertisementTrace(diagnostics, History()).scan(), executor) { 0L }
        var reads = 0
        repeat(256) { index -> capture.record { reads++; raw().copy(address = "$index") } }
        var overflowRead = false
        capture.record { overflowRead = true; raw() }
        executor.runNext()
        assertEquals(1, reads)
        capture.record { reads++; raw().copy(address = "another") }
        capture.failed(7); capture.finish()
        executor.runAll()
        assertFalse(overflowRead); assertEquals(257, reads)
        assertEquals(50, diagnostics.links.count { it.first == "ble_raw" })
        assertEquals(listOf("ble_raw_failed" to mapOf("errorCode" to "7"),
            "ble_raw_dropped" to mapOf("count" to "208")), diagnostics.links.takeLast(2))
        assertTrue(executor.isTerminated())
    }
}
