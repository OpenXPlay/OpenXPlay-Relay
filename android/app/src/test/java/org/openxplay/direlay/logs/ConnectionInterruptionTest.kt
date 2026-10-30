// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-30
package org.openxplay.direlay.logs

import org.junit.Assert.*
import org.junit.Test

class ConnectionInterruptionTest {
    private var now = 100L
    private val timeline = ConnectionTimeline({ now })
    private fun end(link: String = "airplay_control", reached: String = "pair_verified", by: String = "peer",
        cause: String = "peer_eof", facts: Map<String, String> = emptyMap()) =
        LinkEnd(link, reached, by, cause, age = 42, idle = 10, facts = facts)

    @Test fun repeatedVerifyOnlyPeerEofKeepsTheRootAndRebuildDecision() {
        val lines = mutableListOf<String>()
        repeat(3) {
            lines += timeline.phoneStatus("DiscoveringMfi")
            lines += timeline.phone("startup_milestone", "stage=pair_verified")
            now += 42
            lines += timeline.linkEnded("phone", end(facts = mapOf("verifyOnly" to "true", "connection" to "${it + 1}")))
            lines += timeline.linkEnded("phone", end("stream_video", "opening", "local", "teardown"))
            lines += timeline.linkEnded("phone", end("wireless_run", "opening", "local", "rebuild",
                mapOf("next" to "rebuild", "nextInMs" to "0")))
        }
        assertEquals(3, lines.count { "link_ended" in it && "link=airplay_control" in it && "verifyOnly=true" in it })
        val interrupted = lines.filter { it.startsWith("phone interrupted ") }
        assertEquals(3, interrupted.size)
        interrupted.forEachIndexed { index, line ->
            assertTrue(line.contains("attempt=${index + 1}"))
            assertTrue(line.contains("reached=pair_verified link=airplay_control by=peer cause=peer_eof next=rebuild nextInMs=0"))
        }
        assertEquals(3, lines.count { it.startsWith("phone context ") })
    }

    @Test fun bluetoothConnectFailureKeepsMessageAndConsecutiveFailures() {
        timeline.phoneStatus("ConnectingBluetooth")
        val line = timeline.linkEnded("phone", LinkEnd("bluetooth", "connecting", "local", "connect_failed",
            "IOException: read failed, socket might closed", 120, -1, mapOf("consecutiveFailures" to "8", "discovering" to "false"))).single()
        assertTrue(line.contains("link=bluetooth reached=connecting by=local cause=connect_failed age=120 idle=-"))
        assertTrue(line.contains("consecutiveFailures=8"))
        assertTrue(line.contains("detail=\"IOException: read failed, socket might closed\""))
        val lines = timeline.phoneStatus("Failed", "cause=\"Bluetooth connect failed\"").toMutableList()
        assertEquals(2, lines.size)
        assertTrue(lines.any { it.startsWith("phone Failed ") })
        assertTrue(lines.any { it.startsWith("phone failed ") && "reached=connecting_bluetooth cause=\"Bluetooth connect failed\"" in it })
        assertFalse(lines.any { it.startsWith("phone interrupted ") })
        now += 50
        lines += timeline.linkEnded("phone", end("wireless_run", "connecting_bluetooth", "local", "connect_failed",
            mapOf("next" to "rebuild", "nextInMs" to "1500")))
        val interrupted = lines.filter { it.startsWith("phone interrupted ") }
        assertEquals(1, interrupted.size)
        assertTrue(interrupted.single().contains("link=bluetooth by=local cause=connect_failed next=rebuild nextInMs=1500"))
        assertTrue(interrupted.single().contains("detail=\"IOException: read failed, socket might closed\""))
        assertEquals(1, lines.count { it.startsWith("phone context ") })
        now += 1_500
        lines += timeline.phone("attempt_started")
        lines += timeline.phoneStatus("DiscoveringMfi")
        now += 2_000
        lines += timeline.flush()
        assertEquals(1, lines.count { it.startsWith("phone interrupted ") })
        assertFalse(lines.any { "reason=none" in it })
    }

    @Test fun failureWithoutARunEndWaitsTwoSecondsAndKeepsItsFallbackText() {
        timeline.phoneStatus("ConnectingBluetooth")
        val lines = timeline.phoneStatus("Failed", "cause=\"Bluetooth connect failed\"").toMutableList()
        now += 1_999
        assertTrue(timeline.flush().isEmpty())
        now++
        lines += timeline.flush()
        val interrupted = lines.filter { it.startsWith("phone interrupted ") }
        assertEquals(1, interrupted.size)
        assertTrue(interrupted.single().contains("reached=connecting_bluetooth link=wireless_run by=local cause=connect_failed next=none nextInMs=-"))
        assertTrue(interrupted.single().endsWith("detail=\"Bluetooth connect failed\""))
        assertEquals(1, lines.count { it.startsWith("phone context ") })
        lines += timeline.phoneStatus("DiscoveringMfi")
        now += 2_000
        lines += timeline.flush()
        assertEquals(1, lines.count { it.startsWith("phone interrupted ") })
    }

    @Test fun aRunEndReplacesOnlyTheFailureFallbackAndSuppliesTheWaitDecision() {
        for (link in listOf("wireless_run", "usb")) {
            val replay = ConnectionTimeline({ now })
            replay.phone("attempt_started", "link=$link")
            replay.phoneStatus("Failed", "cause=\"Connection failed\"")
            val lines = replay.linkEnded("phone", end(link, "pair_verified", "timeout", "handoff_timeout",
                mapOf("next" to "wait_iphone", "nextInMs" to "1500")))
            val interrupted = lines.single { it.startsWith("phone interrupted ") }
            assertTrue(interrupted.contains("link=$link by=timeout cause=handoff_timeout next=wait_iphone nextInMs=1500"))
            now += 2_000
            assertTrue(replay.flush().isEmpty())
        }
    }

    @Test fun startingARetryDoesNotSummarizeOrLoseAPendingFailure() {
        timeline.phoneStatus("Failed", "cause=\"First attempt failed\"")
        now += 100
        assertTrue(timeline.phone("attempt_started").single().contains("attempt=2"))
        now += 1_900
        val lines = timeline.flush()
        assertEquals(1, lines.count { it.startsWith("phone interrupted attempt=1 ") && "next=none" in it })
        assertTrue(timeline.flush().isEmpty())
    }

    @Test fun teardownCascadesNeverBecomeTheRootAndLocalDecisionIsTheFallback() {
        timeline.phoneStatus("RunningWireless")
        timeline.linkEnded("phone", end("stream_audio", "opening", "local", "teardown"))
        timeline.linkEnded("phone", end("airplay_event", "opening", "local", "teardown"))
        val lines = timeline.phoneDecision(end("wireless_run", "opening", "local", "stopped"), "stop")
        assertTrue(lines.first().contains("link=wireless_run by=local cause=stopped next=stop"))
        assertTrue(timeline.linkEnded("phone", end("wireless_run", "opening", "local", "teardown")).none { it.startsWith("phone interrupted ") })
        assertTrue(timeline.phoneStatus("Failed", "cause=\"stopped\"").none { it.startsWith("phone interrupted ") })
    }

    @Test fun latePeerEndPrecedesTheSessionFallbackAndARebuildCanSupplyTheNextAction() {
        timeline.phone("session_active")
        timeline.phone("session_ended")
        now += 50
        timeline.linkEnded("phone", end())
        now += 1_500
        timeline.phoneStatus("Failed", "cause=\"Session failed\"")
        now += 500
        assertTrue(timeline.flush().isEmpty())
        val lines = timeline.linkEnded("phone", end("wireless_run", "session_active", "local", "rebuild", mapOf("next" to "rebuild")))
        assertTrue(lines.any { it.startsWith("phone interrupted ") && "reached=session_active" in it && "by=peer cause=peer_eof next=rebuild" in it })
        now += 2_000
        assertTrue(timeline.flush().isEmpty())
    }

    @Test fun anActiveSessionEndingWithoutAGenerationDecisionStillHasOneSummary() {
        timeline.phone("session_active")
        timeline.phone("session_ended")
        timeline.linkEnded("phone", end())
        now += 2_000
        val lines = timeline.flush()
        assertEquals(1, lines.count { it.startsWith("phone interrupted ") && "next=none nextInMs=-" in it })
        assertEquals(1, lines.count { it.startsWith("phone context ") })
        assertTrue(timeline.flush().isEmpty())
    }

    @Test fun flappingEndsAreCoalescedAcrossAttemptsAndKeepTheSampleAttempt() {
        var emitted = 0
        repeat(25) {
            timeline.phoneStatus("DiscoveringMfi")
            emitted += timeline.linkEnded("phone", end()).size
            timeline.phoneDecision(end("wireless_run", "opening", "local", "rebuild"), "retry")
            now += 10
        }
        assertEquals(20, emitted)
        timeline.phoneStatus("DiscoveringMfi")
        now = 60_100
        val repeated = timeline.flushEnds().single()
        assertTrue(repeated.contains("attempt=25"))
        assertTrue(repeated.endsWith("repeated=5"))
        assertTrue(timeline.flushEnds().isEmpty())
    }

    @Test fun contextSurvivesEveryUnavailableSourceAndOneBrokenSource() {
        val unavailable = ConnectionContext(ConnectionContext.fields.associateWith { { error("permission denied") } }).line()
        assertEquals(ConnectionContext.fields.joinToString(" ") { "$it=unavailable" }, unavailable)
        val partial = ConnectionContext(mapOf("wifi" to { "connected" }, "screen" to { error("denied") }, "battery" to { "73" })).line()
        assertTrue(partial.contains("wifi=connected")); assertTrue(partial.contains("screen=unavailable")); assertTrue(partial.contains("battery=73"))
        val broken = ConnectionTimeline({ now }, context = { error("context adapter unavailable") })
        val lines = broken.linkEnded("phone", end("usb", "opening", "os", "network_lost"))
        assertEquals("phone context attempt=1 $unavailable", lines.last())
    }

    @Test fun factsAndDetailsAreBoundedPrintableAndCredentialFree() {
        val filtered = LinkEnd("airplay_control", "pair_verified", "peer", "peer_eof",
            "IOException:\nssid=PrivateNetwork mac=AA:BB:CC:DD:EE:FF password=secret body=0123456789abcdef " + "x".repeat(200),
            facts = mapOf("verifyOnly" to "true", "requests" to "3", "deviceName" to "PrivatePhone", "lastRequest" to "AA:BB:CC:DD:EE:FF")).filtered()
        assertEquals(mapOf("verifyOnly" to "true", "requests" to "3"), filtered.facts)
        assertTrue(filtered.detail.length <= 160)
        assertFalse(filtered.detail.contains('\n'))
        listOf("PrivateNetwork", "AA:BB:CC:DD:EE:FF", "secret", "0123456789abcdef").forEach { assertFalse(filtered.line().contains(it)) }
    }

    @Test fun carlinkAndCastConsumeStreamFieldsWithoutInventingEndEventsForOtherDiagnostics() {
        timeline.carlink("link_start", connection = 7)
        timeline.carlink("state=PLAYING", connection = 7)
        val tcp = timeline.carlinkDiagnostic("link_lost link=tcp service=control reached=playing by=timeout cause=health_timeout timeout=heartbeat limitMs=15000", 7)
        assertEquals(1, tcp.count { "carlink link_ended" in it && "link=tcp" in it && "by=timeout cause=health_timeout" in it && "limitMs=15000" in it })
        timeline.carlinkDiagnostic("failed link=session reached=playing by=local cause=teardown", 7)
        val ended = timeline.carlink("state=ENDED", connection = 7).last()
        assertTrue(ended.contains("by=timeout cause=health_timeout reached=playing"))
        assertTrue(timeline.carlink("state=ENDED", connection = 7).isEmpty())
        assertTrue(timeline.carlinkDiagnostic("rtp send stalled ms=100", 7).isEmpty())
        val cast = timeline.cast("request_failed link=renderer reached=http by=timeout cause=read_timeout exception=SocketTimeoutException limitMs=5000").single()
        assertTrue(cast.contains("cast link_ended")); assertTrue(cast.contains("detail=\"SocketTimeoutException limitMs=5000\""))
        assertTrue(timeline.cast("stats clients=1").isEmpty())
    }

    @Test fun gattFailureAndItsCleanupProduceOneEndPerPlatformInstance() {
        timeline.carlink("link_start")
        timeline.carlink("gatt_connect_started")
        val lines = mutableListOf<String>()
        lines += timeline.carlink("gatt_connection status=133 state=disconnected by=os cause=io_error reached=gatt_connect")
        lines += timeline.carlink("ble_exchange failed reason=gatt_connect_failed exception=IllegalStateException by=local cause=connect_failed reached=ble_exchange")
        lines += timeline.carlink("gatt_closed")
        val ends = lines.filter { it.startsWith("carlink link_ended ") }
        assertEquals(1, ends.size)
        assertTrue(ends.single().contains("by=os cause=io_error")); assertTrue(ends.single().contains("status=133"))
        timeline.carlink("gatt_connect_started")
        assertEquals(1, timeline.carlink("gatt_closed").count { it.startsWith("carlink link_ended ") })
    }

    @Test fun legacyFailureDetailsAreNormalizedToCauseAndNeverReasonNone() {
        val lines = timeline.phoneStatus("Failed", "reason=none")
        assertTrue(lines.any { it.startsWith("phone failed ") && "reached=starting cause=\"none\"" in it })
        assertFalse(lines.any { "reason=none" in it })
    }

    @Test fun lateRendererEndsKeepTheRetiredOutputAttempt() {
        timeline.castEvent("output_start", "", 1)
        now += 100
        timeline.castEvent("output_start", "", 2)
        now += 50
        val old = timeline.cast("link_lost link=renderer reached=playing by=local cause=stopped", connection = 1).single()
        val current = timeline.cast("link_lost link=renderer reached=http by=peer cause=peer_eof", connection = 2).single()
        assertTrue(old.contains("attempt=1 t=150"))
        assertTrue(current.contains("attempt=2 t=50"))
    }
}
