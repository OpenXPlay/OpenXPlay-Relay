// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-15
package org.openxplay.direlay.vehicle

import org.junit.Assert.*
import org.junit.Test
import org.junit.Rule
import org.junit.rules.TemporaryFolder
import org.openxplay.stream.core.*

class VehicleRecorderTest {
    @get:Rule val folder = TemporaryFolder()
    private var elapsed = 0L
    private var wall = 1_800_000_000_000L
    private fun advance(ms: Long) { elapsed += ms; wall += ms }
    private fun store() = VehicleSnapshotFile(java.io.File(folder.root, "history.bin"))
    private fun recorder() = VehicleRecorder(store(), { elapsed }, { wall })
    @Test fun cadenceBeginsWithFirstDataAndDoesNotCatchUpWithInventedSnapshots() {
        val recorder = recorder()
        recorder.enable(true); advance(120_000); recorder.tick()
        assertTrue(recorder.view().history.isEmpty())
        recorder.receive(VehicleData.Gear(GearPosition.D, 42u))
        advance(59_999); recorder.tick(); assertTrue(recorder.view().history.isEmpty())
        advance(1); recorder.tick(); assertEquals(1, recorder.view().history.size)
        advance(180_000); recorder.tick(); assertEquals(2, recorder.view().history.size)
        recorder.tick(); assertEquals(2, recorder.view().history.size)
    }
    @Test fun offDiscardsLiveValuesAndRestartsCadenceWithoutRecording() {
        val recorder = recorder()
        recorder.receive(VehicleData.Fuel(100u, 9u)); advance(60_000); recorder.tick()
        assertTrue(recorder.view().reports.isEmpty()); assertTrue(recorder.view().history.isEmpty())
        recorder.enable(true); recorder.receive(VehicleData.Fuel(100u, 42u)); advance(30_000)
        recorder.enable(false); advance(60_000); recorder.tick()
        recorder.receive(VehicleData.Gear(GearPosition.P, 0u))
        assertTrue(recorder.view().history.isEmpty()); assertTrue(recorder.view().reports.isEmpty())
        recorder.enable(true); advance(60_000); recorder.tick(); assertTrue(recorder.view().history.isEmpty())
        recorder.receive(VehicleData.Gear(GearPosition.R, 2u)); advance(60_000); recorder.tick()
        assertEquals(listOf("gear"), recorder.view().history.single().reports.map { it.type })
    }
    @Test fun newestThirtySurviveRestartAndClearIsPersistent() {
        val recorder = recorder(); recorder.enable(true)
        repeat(35) { index ->
            recorder.receive(VehicleData.Fuel(UInt.MAX_VALUE, index.toUInt()))
            advance(60_000); recorder.tick()
        }
        val history = recorder.view().history
        assertEquals(30, history.size)
        assertEquals("34", history.first().reports.single().fields["current"])
        assertEquals("5", history.last().reports.single().fields["current"])
        assertEquals(history, recorder().view().history)
        assertFalse(recorder().view().enabled)
        recorder.clear(); assertTrue(recorder().view().history.isEmpty())
    }
    @Test fun latestOfEveryTypeIsSavedAndUnsignedValuesRoundTrip() {
        val recorder = recorder(); recorder.enable(true)
        val data = listOf<VehicleData>(
            VehicleData.Gps(-12, Int.MIN_VALUE, Int.MAX_VALUE, UInt.MAX_VALUE, 9u, UInt.MAX_VALUE, 360u, 8u, 12u, ULong.MAX_VALUE),
            VehicleData.Lights(true, false, true, false, true),
            VehicleData.Gyroscope(GyroscopeType.THREE_AXIS, -12.5, 3.25, 270.5, ULong.MAX_VALUE),
            VehicleData.Acceleration(-1.5, 2.25, 9.75, 3uL),
            VehicleData.Fuel(UInt.MAX_VALUE, 42u), VehicleData.Gear(GearPosition.D, 72u),
            VehicleData.LightSensor(1000.5, .25, 50.75), VehicleData.DayNight(DayNightMode.NIGHT))
        data.forEach(recorder::receive)
        recorder.receive(VehicleData.Fuel(UInt.MAX_VALUE, 99u))
        advance(60_000); recorder.tick()
        val snapshot = store().read().single()
        assertEquals(8, snapshot.reports.size)
        assertEquals(ULong.MAX_VALUE.toString(), snapshot.reports.first { it.type == "gps" }.fields["timestamp"])
        assertEquals("99", snapshot.reports.first { it.type == "fuel" }.fields["current"])
        assertEquals("-12.5", snapshot.reports.first { it.type == "gyroscope" }.fields["x"])
    }
    @Test fun clockCorrectionsDoNotBreakCadenceOrDuplicateSnapshotIdentities() {
        val recorder = recorder(); recorder.enable(true); recorder.receive(VehicleData.Fuel(100u, 42u))
        advance(60_000); recorder.tick()
        wall -= 120_000; elapsed += 60_000; recorder.tick()
        val history = recorder.view().history
        assertEquals(2, history.size)
        assertTrue(history.first().time < history.last().time)
        assertTrue(history.first().id > history.last().id)
        assertEquals(history, store().read())
    }
    @Test fun corruptHistoryCanBeClearedAndFailedAtomicWritePreservesLastGeneration() {
        val target = java.io.File(folder.root, "history.bin")
        target.writeBytes(byteArrayOf(0, 1))
        val recorder = recorder()
        assertTrue(recorder.view().storageFailed)
        recorder.clear(); assertFalse(recorder.view().storageFailed)
        recorder.enable(true); recorder.receive(VehicleData.Fuel(100u, 42u))
        advance(60_000); recorder.tick()
        val previous = store().read()
        val staging = java.io.File(folder.root, "history.bin.tmp")
        assertTrue(staging.mkdir())
        advance(60_000)
        try { recorder.tick(); fail("Expected failed write") } catch (_: java.io.IOException) { }
        assertEquals(previous, store().read())
    }
    @Test fun tileStatesAndGlanceUseNewestRawSpeed() {
        assertEquals(VehicleTileState.OFF, vehicleTileState(VehicleView()))
        assertEquals(VehicleTileState.WAITING, vehicleTileState(VehicleView(enabled = true)))
        val reports = listOf(vehicleReport(VehicleData.Gear(GearPosition.D, 72u), 1),
            vehicleReport(VehicleData.Gps(0, 0, 0, 0u, 0u, 80u, 0u, 0u, 0u, 0uL), 2), vehicleReport(VehicleData.Fuel(100u, 42u), 2))
        assertEquals(VehicleTileState.DATA, vehicleTileState(VehicleView(true, reports)))
        assertEquals(listOf("80", "D", "42"), vehicleGlance(reports))
        assertEquals(listOf("—", "—", "—"), vehicleGlance(emptyList()))
    }

    @Test fun deterministicSamplesUseRealReportsButNeverCreateOrReplaceHistory() {
        val recorder = recorder()
        recorder.enable(true)
        recorder.receive(VehicleData.Fuel(100u, 12u))
        advance(60_000); recorder.tick()
        val previous = store().read()
        val bytes = java.io.File(folder.root, "history.bin").readBytes()
        recorder.enable(true, sample = true)
        assertTrue(recorder.view().sample)
        assertTrue(recorder.view().reports.isEmpty())
        assertEquals(sampleVehicleData(17), sampleVehicleData(17))
        assertNotEquals(sampleVehicleData(17), sampleVehicleData(18))
        repeat(130) { tick ->
            sampleVehicleData(tick.toLong()).forEach { recorder.receive(it) }
            advance(1000); recorder.tick()
        }
        assertEquals(8, recorder.view().reports.size)
        assertEquals(previous, recorder.view().history)
        assertArrayEquals(bytes, java.io.File(folder.root, "history.bin").readBytes())
        assertEquals(previous, recorder().view().history)
        recorder.enable(true, sample = false)
        assertFalse(recorder.view().sample)
        assertTrue(recorder.view().reports.isEmpty())
        advance(60_000); recorder.tick()
        assertEquals(previous, store().read())
        recorder.receive(VehicleData.Gear(GearPosition.P, 0u))
        advance(60_000); recorder.tick()
        assertEquals(listOf("gear"), store().read().first().reports.map { it.type })
    }
    @Test fun sampleOnlySessionNeverCreatesAHistoryFile() {
        val recorder = recorder()
        recorder.enable(true, sample = true)
        sampleVehicleData(0).forEach { recorder.receive(it) }
        advance(120_000); recorder.tick()
        assertFalse(java.io.File(folder.root, "history.bin").exists())
        assertEquals(VehicleTileState.DATA, vehicleTileState(recorder.view()))
        recorder.enable(false)
        assertFalse(recorder.view().sample)
        assertEquals(VehicleTileState.OFF, vehicleTileState(recorder.view()))
    }
}
