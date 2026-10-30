// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-23
package org.openxplay.direlay.logs

import org.junit.Assert.*
import org.junit.Test

class ConnectionTimelineTest {
    private var now = 100L
    private val timeline = ConnectionTimeline({ now })
    @Test fun monotonicFormatAndIndependentDomains() {
        assertEquals("relay process_start attempt=1 t=0 dt=0", timeline.event("relay", "process_start").single())
        now = 125
        assertEquals("network wifi_enabled attempt=1 t=25 dt=25", timeline.event("network", "wifi_enabled").single())
        now = 180
        assertEquals("relay app_ready attempt=1 t=80 dt=80 duration=80", timeline.event("relay", "app_ready").single())
        now = 170
        assertTrue(timeline.event("relay", "late").single().endsWith("t=70 dt=0"))
    }
    @Test fun transitionsAreOncePerChange() {
        assertEquals(1, timeline.transition("network", "wifi", "wifi_enabled").size)
        assertTrue(timeline.transition("network", "wifi", "wifi_enabled").isEmpty())
        assertEquals(1, timeline.transition("network", "wifi", "wifi_disabled").size)
        assertEquals(1, timeline.transition("network", "wifi", "wifi_enabled").size)
    }
    @Test fun phonePhasesAndFirstVideoAreSummarizedOnce() {
        timeline.phoneStatus("DiscoveringMfi")
        assertTrue(timeline.phoneStatus("DiscoveringMfi").isEmpty())
        now = 110; timeline.phoneStatus("MfiReady")
        now = 115; timeline.phoneStatus("StartingHotspot")
        now = 135; timeline.phoneStatus("HotspotReady", "band=5GHz channel=44")
        now = 140; timeline.phoneStatus("ConnectingBluetooth")
        now = 170; timeline.phoneStatus("RunningWireless")
        now = 200; timeline.phoneStatus("WirelessActive")
        now = 220
        val summary = timeline.phone("first_video", "codec=H264 size=1280x720").last()
        assertEquals("phone connected attempt=1 t=120 dt=0 total=120 mfi=10 hotspot=20 bluetooth=30 wireless=30 first_video=120", summary)
        assertTrue(timeline.phone("first_video").isEmpty())
        assertEquals(1, timeline.phone("session_active").size)
        now = 320
        assertTrue(timeline.phone("session_ended").single().endsWith("duration=100"))
    }
    @Test fun phoneFailureRetryAndUsbMissingPhases() {
        timeline.phoneStatus("DiscoveringMfi")
        now = 120; assertTrue(timeline.phoneStatus("Failed", "cause=\"mfi_timeout\"").any { it.contains("phone failed attempt=1") })
        now = 130; assertTrue(timeline.phoneStatus("DiscoveringIphone").single().contains("attempt=2 t=0 dt=0"))
        now = 140
        val summary = timeline.phone("session_active").last()
        assertTrue(summary.endsWith("total=10")); assertFalse(summary.contains("hotspot="))
    }
    @Test fun carlinkPhasesPlayingAndEnded() {
        timeline.carlink("link_start")
        val script = listOf(110L to "ble_scan_started", 130L to "ble_peer_found", 140L to "gatt_connect_started",
            170L to "gatt_connected", 175L to "ble_exchange", 215L to "ble_exchange_done frequency=5180",
            220L to "wifi_join", 270L to "wifi_join_success peer=192.168.1.1 local=192.168.1.2",
            280L to "tcp_connect_started", 340L to "tcp_connected", 410L to "auth_success",
            415L to "state=NEGOTIATING", 420L to "first_video codec=H264")
        script.forEach { (at, line) -> now = at; timeline.carlink(line) }
        now = 500
        assertEquals("carlink connected attempt=1 t=400 dt=0 total=400 ble_scan=20 gatt=30 ble_exchange=40 wifi_join=50 tcp=60 auth=70 negotiate=85 first_video=320",
            timeline.carlink("state=PLAYING").last())
        assertTrue(timeline.carlink("state=PLAYING").isEmpty())
        now = 900; timeline.carlink("session failed reason=heartbeat_timeout exception=IOException")
        now = 1000
        assertTrue(timeline.carlink("state=ENDED").last().contains("played=500 reason=heartbeat_timeout by=timeout cause=health_timeout"))
    }
    @Test fun carlinkRetryCountsLinkStartsAndOmitsMissingPhases() {
        timeline.carlink("state=CONNECTING")
        assertTrue(timeline.carlink("link_start").single().contains("attempt=1"))
        now = 200; timeline.carlink("ble_scan failed reason=ble_timeout exception=IOException")
        now = 300; assertTrue(timeline.carlink("state=ENDED").last().contains("played=0 reason=ble_timeout by=timeout cause=read_timeout"))
        assertTrue(timeline.carlink("link_retry delayMs=1000").single().contains("attempt=1"))
        now = 1400; assertTrue(timeline.carlink("link_start").single().contains("attempt=2 t=0 dt=0"))
        now = 1450
        val summary = timeline.carlink("state=PLAYING").last()
        assertTrue(summary.endsWith("total=50")); assertFalse(summary.contains("wifi_join="))
    }
    @Test fun delayedSummaryStaysWithItsConnectionAfterAReconnect() {
        timeline.carlink("link_start", connection = 10)
        now = 200; timeline.carlink("state=ENDED", connection = 10)
        now = 300; timeline.carlink("link_start", connection = 11)
        now = 400
        assertTrue(timeline.carlink("car vendor=Vendor", connection = 10).single().contains("attempt=1 t=300"))
        assertTrue(timeline.carlink("state=PLAYING", connection = 11).last().contains("attempt=2 t=100"))
    }
}
