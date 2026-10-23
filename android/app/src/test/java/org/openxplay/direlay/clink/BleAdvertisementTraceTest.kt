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
    private fun raw() = BleRawAdvertisement("AA:BB:CC:DD:EE:FF", 1, -71, true, false, -8, "Nearby device",
        listOf(BleCodec.ADVERTISEMENT_SERVICE), mapOf(BleCodec.NAME_SERVICE to byteArrayOf(0, 0x7f, 0xff.toByte())),
        mapOf(76 to byteArrayOf(0xab.toByte(), 0x01)), byteArrayOf(0, 0x80.toByte(), 0xff.toByte()))

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
}
