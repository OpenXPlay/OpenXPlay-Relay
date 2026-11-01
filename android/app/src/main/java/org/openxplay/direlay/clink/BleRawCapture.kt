// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-31
package org.openxplay.direlay.clink

import android.bluetooth.le.ScanResult
import android.os.SystemClock
import java.util.concurrent.ExecutorService
import java.util.concurrent.Executors

internal class BleRawCapture(private val scan: BleAdvertisementTrace.Scan,
    private val executor: ExecutorService = Executors.newSingleThreadExecutor { task ->
        Thread(task, "clink-ble-raw").apply { isDaemon = true }
    }, private val nowMs: () -> Long = { SystemClock.elapsedRealtime() }) {
    private var pending = 0
    private var finished = false

    fun record(result: ScanResult) { record { BleRawAdvertisement.from(result) } }
    @Synchronized fun record(read: () -> BleRawAdvertisement) {
        if (finished) return
        if (pending >= 256) { scan.dropped(); return }
        pending++
        executor.execute {
            synchronized(this) { pending-- }
            try { scan.record(read(), nowMs()) }
            catch (_: Exception) { scan.failed(-1) }
        }
    }
    @Synchronized fun failed(errorCode: Int) {
        if (!finished) executor.execute { scan.failed(errorCode) }
    }
    @Synchronized fun finish() {
        if (finished) return
        finished = true
        try { executor.execute { scan.finish() } }
        finally { executor.shutdown() }
    }
}
