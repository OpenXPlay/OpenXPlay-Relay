// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-15
package org.openxplay.direlay.vehicle

import org.openxplay.stream.core.*
import java.io.*
import java.nio.file.Files
import java.nio.file.StandardCopyOption.*

/** Raw wire values, including unsigned ranges, are never converted to inferred units. */
data class VehicleReport(val type: String, val fields: Map<String, String>, val receivedAt: Long)
data class VehicleSnapshot(val time: Long, val reports: List<VehicleReport>, val id: Long = time)
data class VehicleView(val enabled: Boolean = false, val reports: List<VehicleReport> = emptyList(),
    val history: List<VehicleSnapshot> = emptyList(), val storageFailed: Boolean = false,
    val sample: Boolean = false)
fun vehicleReport(data: VehicleData, time: Long): VehicleReport {
    fun fields(vararg pairs: Pair<String, Any>) = pairs.associate { it.first to it.second.toString() }
    val (type, values) = when (data) {
        is VehicleData.Gps -> "gps" to fields("latitude" to data.latitude, "longitude" to data.longitude,
            "altitude" to data.height, "speed" to data.speed, "heading" to data.heading,
            "antenna" to data.antennaState, "pdop" to data.pdop, "used" to data.satellitesUsed,
            "visible" to data.satellitesVisible, "timestamp" to data.timestamp)
        is VehicleData.Lights -> "lights" to fields("low" to data.lowBeam, "high" to data.highBeam,
            "position" to data.positionLamp, "reverse" to data.reverseLamp, "brake" to data.brakeLamp)
        is VehicleData.Gear -> "gear" to fields("gear" to data.gear, "speed" to data.speed)
        is VehicleData.Fuel -> "fuel" to fields("max" to data.max, "current" to data.current)
        is VehicleData.Gyroscope -> "gyroscope" to fields("type" to data.type, "x" to data.x,
            "y" to data.y, "z" to data.z, "timestamp" to data.timestamp)
        is VehicleData.Acceleration -> "acceleration" to fields("x" to data.x, "y" to data.y,
            "z" to data.z, "timestamp" to data.timestamp)
        is VehicleData.LightSensor -> "light" to fields("max" to data.maxLux, "min" to data.minLux, "current" to data.currentLux)
        is VehicleData.DayNight -> "daynight" to fields("mode" to data.mode)
    }
    return VehicleReport(type, values, time)
}

/** Private, versioned binary format; publish only after the complete file is synced. */
class VehicleSnapshotFile(private val file: File) {
    fun read(): List<VehicleSnapshot> {
        if (!file.exists()) return emptyList()
        DataInputStream(BufferedInputStream(FileInputStream(file))).use { input ->
            val version = input.readInt().also { require(it in 1..2) }
            val count = input.readInt().also { require(it in 0..30) }
            return List(count) {
                val time = input.readLong()
                val id = if (version == 2) input.readLong() else time
                val size = input.readInt().also { require(it in 0..8) }
                VehicleSnapshot(time, List(size) {
                    val type = input.readUTF()
                    val at = input.readLong()
                    val fields = input.readInt().also { require(it in 0..16) }
                    VehicleReport(type, buildMap { repeat(fields) { put(input.readUTF(), input.readUTF()) } }, at)
                }, id)
            }
        }
    }
    fun write(snapshots: List<VehicleSnapshot>) {
        require(snapshots.size <= 30)
        file.parentFile?.mkdirs()
        val staging = File(file.parentFile, file.name + ".tmp")
        try {
            FileOutputStream(staging).use { stream ->
                val out = DataOutputStream(stream)
                out.writeInt(2); out.writeInt(snapshots.size)
                snapshots.forEach { snapshot ->
                    out.writeLong(snapshot.time); out.writeLong(snapshot.id); out.writeInt(snapshot.reports.size)
                    snapshot.reports.forEach { report ->
                        out.writeUTF(report.type); out.writeLong(report.receivedAt); out.writeInt(report.fields.size)
                        report.fields.forEach { (key, value) -> out.writeUTF(key); out.writeUTF(value) }
                    }
                }
                out.flush(); stream.fd.sync()
            }
            Files.move(staging.toPath(), file.toPath(), ATOMIC_MOVE, REPLACE_EXISTING)
        } finally { staging.delete() }
    }
}

/** Called on one runtime worker. Cadence uses monotonic time; display times use wall time. */
class VehicleRecorder(private val file: VehicleSnapshotFile,
    private val elapsed: () -> Long, private val wall: () -> Long) {
    private var enabled = false
    private var sampleSession = false
    private var next: Long? = null
    private val latest = linkedMapOf<String, VehicleReport>()
    private var storageFailed = false
    private var history = try { file.read() } catch (_: Exception) { storageFailed = true; emptyList() }
    fun enable(value: Boolean, sample: Boolean = false) {
        if (enabled == value && sampleSession == sample) return
        sampleSession = sample
        enabled = value; next = null; latest.clear()
    }
    fun receive(data: VehicleData, receivedAt: Long = wall()) {
        if (!enabled) return
        val report = vehicleReport(data, receivedAt)
        latest[report.type] = report
        if (next == null) next = elapsed() + 60_000
    }
    fun tick() {
        val due = next ?: return
        if (!enabled || sampleSession || latest.isEmpty() || elapsed() < due) return
        val at = wall()
        // Stable identities also cover wall-clock corrections while the monotonic cadence continues.
        val id = maxOf(at, (history.firstOrNull()?.id ?: 0L) + 1)
        val updated = (listOf(VehicleSnapshot(at, latest.values.toList(), id)) + history).take(30)
        file.write(updated)
        history = updated
        storageFailed = false
        // A delayed worker records once, without inventing samples for missed intervals.
        next = elapsed() + 60_000
    }
    fun clear() { file.write(emptyList()); history = emptyList(); storageFailed = false }
    fun view() = VehicleView(enabled, latest.values.toList(), history, storageFailed, sampleSession)
}

enum class VehicleTileState { OFF, WAITING, DATA }
fun vehicleTileState(view: VehicleView) = when {
    !view.enabled -> VehicleTileState.OFF
    view.reports.isEmpty() -> VehicleTileState.WAITING
    else -> VehicleTileState.DATA
}
fun vehicleGlance(reports: List<VehicleReport>): List<String> {
    fun field(type: String, key: String) = reports.firstOrNull { it.type == type }?.fields?.get(key)
    val speed = reports.filter { "speed" in it.fields }.maxByOrNull { it.receivedAt }?.fields?.get("speed") ?: "—"
    return listOf(speed, field("gear", "gear") ?: "—", field("fuel", "current") ?: "—")
}
