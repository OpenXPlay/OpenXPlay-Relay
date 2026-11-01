// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-23
package org.openxplay.direlay.clink

import android.annotation.SuppressLint
import android.bluetooth.le.ScanResult
import android.os.Build
import org.json.JSONObject
import org.openxplay.stream.clink.ble.Advertisement
import org.openxplay.stream.clink.ble.BleCodec
import java.util.concurrent.atomic.AtomicLong

/** Bound duplicate memory as well as the trace rate during a continuous scan. */
internal class BleRawThrottle {
    private val recent = linkedMapOf<Pair<String, String>, Long>()
    private val emitted = java.util.ArrayDeque<Long>()
    private val droppedCount = AtomicLong()
    val dropped: Long get() = droppedCount.get()
    fun drop() { droppedCount.incrementAndGet() }

    @Synchronized fun accept(address: String, payload: String, nowMs: Long): Boolean {
        while (emitted.isNotEmpty() && nowMs - emitted.first >= 1000) emitted.removeFirst()
        val iterator = recent.iterator()
        while (iterator.hasNext()) if (nowMs - iterator.next().value >= 2000) iterator.remove()
        val key = address to payload
        if (key in recent || emitted.size >= 50) { drop(); return false }
        recent[key] = nowMs
        emitted.addLast(nowMs)
        return true
    }
}

internal data class BleRawAdvertisement(
    val address: String, val addressType: Int?, val rssi: Int, val connectable: Boolean,
    val legacy: Boolean, val txPower: Int, val localName: String?, val serviceUuids: List<String>,
    val serviceData: Map<String, ByteArray>, val manufacturerData: Map<Int, ByteArray>, val raw: ByteArray,
) {
    fun fields(rawHex: String = hex(raw)): Map<String, String> = buildMap {
        put("address", address)
        addressType?.let { put("addressType", it.toString()) }
        put("rssi", rssi.toString()); put("connectable", connectable.toString())
        put("legacy", legacy.toString()); put("txPower", txPower.toString())
        put("localName", localName.orEmpty())
        put("serviceUuids", org.json.JSONArray(serviceUuids).toString())
        put("serviceData", JSONObject().also { json -> serviceData.toSortedMap().forEach { (uuid, bytes) -> json.put(uuid, hex(bytes)) } }.toString())
        put("manufacturerData", JSONObject().also { json -> manufacturerData.toSortedMap().forEach { (id, bytes) -> json.put(id.toString(), hex(bytes)) } }.toString())
        put("raw", rawHex)
    }

    companion object {
        private const val HEX = "0123456789abcdef"
        fun hex(bytes: ByteArray): String {
            val chars = CharArray(bytes.size * 2)
            bytes.forEachIndexed { index, byte ->
                val value = byte.toInt() and 255
                chars[index * 2] = HEX[value ushr 4]; chars[index * 2 + 1] = HEX[value and 15]
            }
            return String(chars)
        }

        @SuppressLint("MissingPermission")
        fun from(scan: ScanResult): BleRawAdvertisement {
            val record = scan.scanRecord
            val manufacturers = record?.manufacturerSpecificData
            val txPower = scan.txPower.takeUnless { it == ScanResult.TX_POWER_NOT_PRESENT }
                ?: record?.txPowerLevel?.takeUnless { it == Int.MIN_VALUE } ?: ScanResult.TX_POWER_NOT_PRESENT
            return BleRawAdvertisement(scan.device.address,
                if (Build.VERSION.SDK_INT >= 35) scan.device.addressType else null,
                scan.rssi, scan.isConnectable, scan.isLegacy, txPower, record?.deviceName,
                record?.serviceUuids.orEmpty().map { it.toString() },
                record?.serviceData.orEmpty().mapKeys { it.key.toString() },
                buildMap { if (manufacturers != null) for (i in 0 until manufacturers.size())
                    put(manufacturers.keyAt(i), manufacturers.valueAt(i)) }, record?.bytes ?: byteArrayOf())
        }
    }
}

/** Exists only for E3; none of its history or scan work runs on the ordinary path. */
internal class BleAdvertisementTrace(private val diagnostics: ClinkDiagnostics, private val store: Store) {
    interface Store {
        fun readSerial(carId: String): Int?
        fun writeSerial(carId: String, serial: Int)
    }
    private var selected: Advertisement? = null

    fun serialFields(advertisement: Advertisement): Map<String, String> = mapOf(
        "carId" to advertisement.carId, "serial" to advertisement.serial.toString(),
        "serialSame" to (store.readSerial(advertisement.carId) == advertisement.serial).toString())

    @Synchronized fun selected(advertisement: Advertisement) { selected = advertisement }
    @Synchronized fun connected() {
        selected?.let { store.writeSerial(it.carId, it.serial) }
        selected = null
    }
    fun sessionDiagnostic(message: String) {
        if (message.startsWith("auth ")) diagnostics.emit(ClinkDiagnostic.Auth(message))
    }
    fun scan() = Scan()

    inner class Scan {
        private val throttle = BleRawThrottle()
        private var finished = false

        fun dropped() { throttle.drop() }
        @Synchronized fun record(raw: BleRawAdvertisement, nowMs: Long) {
            if (finished) return
            val rawHex = BleRawAdvertisement.hex(raw.raw)
            if (!throttle.accept(raw.address, rawHex, nowMs)) return
            val fields = raw.fields(rawHex).toMutableMap()
            val identity = raw.serviceData[BleCodec.IDENTITY_SERVICE]
            if (identity != null) try {
                val advertisement = BleCodec.advertisement(identity, raw.serviceData[BleCodec.NAME_SERVICE] ?: byteArrayOf())
                fields.putAll(serialFields(advertisement))
            } catch (_: IllegalArgumentException) { /* Preserve malformed advertisements as raw records. */ }
            diagnostics.traceLink("ble_raw", fields)
        }
        @Synchronized fun failed(errorCode: Int) {
            if (!finished) diagnostics.traceLink("ble_raw_failed", mapOf("errorCode" to errorCode.toString()))
        }
        @Synchronized fun finish() {
            if (finished) return
            finished = true
            diagnostics.traceLink("ble_raw_dropped", mapOf("count" to throttle.dropped.toString()))
        }
    }
}
