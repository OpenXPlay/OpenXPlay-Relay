// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-22
package org.openxplay.direlay.relay

import com.shilapi.xcertplay.iap2.wire.Iap2Frame
import com.shilapi.xcertplay.network.WirelessStartupFailure
import com.shilapi.xcertplay.orchestration.CarPlayStatus
import org.junit.Assert.*
import org.junit.Test
import java.time.ZoneId
import java.util.concurrent.Executors
import java.util.concurrent.TimeUnit

class CarPlayDiagnosticsTest {
    private fun diagnostics(secrets: () -> Collection<String> = { emptyList() }) =
        CarPlayDiagnostics(secrets, StepTrail(ZoneId.of("UTC")), { 123L }, { 0L })

    @Test fun discoveryStartsANewAttemptButLeavesTheFrozenErrorUntilRecovery() {
        val observer = diagnostics()
        observer.onProgress("old attempt")
        observer.onStatus(CarPlayStatus.Failed("first"))
        val first = observer.error
        observer.onStatus(CarPlayStatus.DiscoveringMfi)
        observer.onProgress("new attempt")
        assertSame(first, observer.error)
        observer.onStatus(CarPlayStatus.Failed("second", true, WirelessStartupFailure.FIRST_TCP_TIMEOUT))
        val error = observer.error!!
        assertEquals("second", error.text)
        assertEquals("first_tcp_timeout", error.code)
        assertEquals(123L, error.atMillis)
        assertEquals(listOf("00:00:00.123 status=DiscoveringMfi", "+0.000s progress new attempt",
            "+0.000s status=Failed cause=\"second\" startupFailure=FIRST_TCP_TIMEOUT wifiResetRequired=true"), error.trail)
        observer.onSessionEnded()
        assertEquals(3, error.trail.size)
        observer.onSessionActive()
        assertNull(observer.error)
        observer.onStatus(CarPlayStatus.Failed("third"))
        assertTrue(observer.error!!.trail.any { it.endsWith("session_ended") })
        assertTrue(observer.error!!.trail.any { it.endsWith("session_active") })
        observer.clear()
        assertNull(observer.error)
        observer.onStatus(CarPlayStatus.Failed("fourth"))
        assertNull(observer.error!!.code)
        assertTrue(observer.error!!.trail.any { it.endsWith("session_active") })
    }

    @Test fun hotspotKeepsOnlyBackendBandAndChannel() {
        val observer = diagnostics()
        observer.onStatus(CarPlayStatus.HotspotReady(ssid = "PrivateNetwork", band = "5GHz", channel = 36,
            bssid = "AA:BB:CC:DD:EE:FF", address = "192.168.1.1", backend = "p2p"))
        observer.onStatus(CarPlayStatus.Failed("failed"))
        assertEquals("00:00:00.123 status=HotspotReady backend=p2p band=5GHz channel=36", observer.error!!.trail.first())
        val trail = observer.error!!.trail.joinToString()
        listOf("PrivateNetwork", "AA:BB:CC:DD:EE:FF", "192.168.1.1").forEach { assertFalse(trail.contains(it)) }
    }

    @Test fun redactsCurrentSecretsAndDropsUnsafeProgressBeforeRecording() {
        var current = listOf("wifi-secret", "remote-secret")
        val observer = diagnostics { current }
        observer.onProgress("accepted wifi-secret remote-secret")
        observer.onProgress("password=labelled-secret pin=1234")
        observer.onProgress("body=0123456789abcdef")
        observer.onProgress("X".repeat(161))
        observer.onProgress("non-ascii é")
        observer.onProgress("line\nbreak")
        observer.onProgress("control\tcharacter")
        observer.onProgress("X".repeat(160))
        current = listOf("updated-secret")
        observer.onStatus(CarPlayStatus.Failed("failed updated-secret token=labelled-token"))
        val error = observer.error!!
        assertEquals("failed [redacted] token=[redacted]", error.text)
        assertEquals(4, error.trail.size)
        assertTrue(error.trail[0].endsWith("progress accepted [redacted] [redacted]"))
        assertTrue(error.trail[1].endsWith("progress password=[redacted] pin=[redacted]"))
        assertTrue(error.trail[2].endsWith("progress " + "X".repeat(160)))
        listOf("wifi-secret", "remote-secret", "labelled-secret", "labelled-token", "updated-secret").forEach {
            assertFalse(error.toString().contains(it))
        }
    }

    @Test fun commandParametersAreIgnoredAndTypeIsReducedToAsciiIdentifiers() {
        val observer = diagnostics()
        observer.onIap2Frame(Iap2Frame(1, "private-body".toByteArray()))
        observer.onCommand("request-UI! é\n", mapOf("password" to "private-parameter"))
        observer.onStatus(CarPlayStatus.Failed("failed"))
        assertEquals("00:00:00.123 command type=requestUI", observer.error!!.trail.first())
        assertEquals(2, observer.error!!.trail.size)
        assertFalse(observer.error!!.trail.joinToString().contains("private-parameter"))
        assertFalse(observer.error!!.trail.joinToString().contains("private-body"))
    }

    @Test fun concurrentCallbacksAndSnapshotsStayBoundedAndComplete() {
        val observer = diagnostics()
        val workers = Executors.newFixedThreadPool(4)
        try {
            val tasks = (0..3).map { worker -> workers.submit {
                repeat(200) {
                    when (worker) {
                        0 -> observer.onStatus(CarPlayStatus.DiscoveringMfi)
                        1 -> observer.onProgress("connecting")
                        2 -> { observer.onSessionActive(); observer.onSessionEnded() }
                        else -> { observer.onStatus(CarPlayStatus.Failed("failed")); observer.error?.let { error ->
                            assertTrue(error.trail.size <= 80)
                            assertTrue(error.trail.last().contains("status=Failed"))
                        } }
                    }
                }
            } }
            tasks.forEach { it.get(10, TimeUnit.SECONDS) }
            observer.onStatus(CarPlayStatus.Failed("final"))
            assertEquals("final", observer.error!!.text)
            assertTrue(observer.error!!.trail.all { it.length <= 200 })
        } finally { workers.shutdownNow() }
    }
}
