// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-23
package org.openxplay.direlay.logs

import org.junit.Assert.*
import org.junit.Test
import java.util.concurrent.CountDownLatch
import java.util.concurrent.Executors
import java.util.concurrent.TimeUnit

class BoundedLogWorkerTest {
    @Test fun producersNeverWaitAndOverloadIsCounted() {
        val entered = CountDownLatch(1)
        val release = CountDownLatch(1)
        val worker = BoundedLogWorker("test-log", capacity = 2)
        val producer = Executors.newSingleThreadExecutor()
        try {
            worker.offer { entered.countDown(); release.await() }
            assertTrue(entered.await(2, TimeUnit.SECONDS))
            producer.submit { repeat(100) { worker.offer { } } }.get(2, TimeUnit.SECONDS)
            assertEquals(99L, worker.dropped.get())
        } finally { release.countDown(); worker.close(); producer.shutdownNow() }
    }
    @Test fun brokenSinksDoNotKillTheWorkerAndWarnOnlyOnce() {
        val warnings = mutableListOf<String>()
        val worker = BoundedLogWorker("test-log", warning = { warnings += it })
        try {
            repeat(3) { worker.offer { error("broken") } }
            worker.awaitIdle()
            assertEquals(listOf("logging_failed"), warnings)
        } finally { worker.close() }
    }
    @Test fun everyStreamDiagnosticReachesTheLogRegardlessOfPrefix() {
        val actual = mutableListOf<String>()
        val log = ConnectionLog(clock = { 10 }, write = { tag, line -> if (tag == "DiRelayCarlink") actual += line })
        try {
            val messages = listOf("link stalled", "link recovered", "RTP send stalled", "auth re-pin reason=peer_key_changed",
                "unsupported audio", "malformed_day_night", "video reset", "audio queue drops=3", "frame record_skipped")
            messages.forEach(log::diagnostic)
            log.awaitIdle()
            assertEquals(messages, actual)
        } finally { log.close() }
    }
}
