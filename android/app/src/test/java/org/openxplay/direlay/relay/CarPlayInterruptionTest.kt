// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-30
package org.openxplay.direlay.relay

import android.app.Application
import com.shilapi.xcertplay.orchestration.CarPlayLinkEnd
import com.shilapi.xcertplay.orchestration.CarPlayStatus
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import org.openxplay.direlay.logs.ConnectionLog
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [33], manifest = Config.NONE, application = Application::class)
class CarPlayInterruptionTest {
    @Test fun observerMapsTheSeamAndAccountsMilestonesWithStartupDiagnosticsOff() {
        val lines = mutableListOf<String>()
        val log = ConnectionLog(clock = { 100 }, write = { _, line -> lines += line })
        try {
            val observer = CarPlayDiagnostics(secrets = { listOf("private-password") }, timeline = log)
            observer.onStatus(CarPlayStatus.DiscoveringMfi)
            observer.onStartupMilestone("pair_verified")
            observer.onLinkEnded(CarPlayLinkEnd("airplay_control", "pair_verified", "peer", "peer_eof",
                "IOException: private-password\nmac=AA:BB:CC:DD:EE:FF", 75, 12,
                mapOf("verifyOnly" to "true", "connection" to "32", "deviceName" to "PrivatePhone")))
            observer.onStatus(CarPlayStatus.Failed("Bluetooth read failed private-password"))
            observer.onLinkEnded(CarPlayLinkEnd("wireless_run", "pair_verified", "peer", "peer_eof",
                facts = mapOf("next" to "rebuild", "nextInMs" to "1500")))
            log.awaitIdle()
            assertEquals(1, lines.count { it.startsWith("phone interrupted ") && "reached=pair_verified" in it && "by=peer cause=peer_eof next=rebuild nextInMs=1500" in it })
            assertTrue(lines.any { "link=airplay_control" in it && "age=75 idle=12" in it && "verifyOnly=true" in it })
            assertTrue(lines.any { it.startsWith("phone failed ") && "cause=\"Bluetooth read failed [redacted]\"" in it })
            assertEquals(1, lines.count { it.startsWith("phone context ") })
            listOf("private-password", "PrivatePhone", "AA:BB:CC:DD:EE:FF", "reason=none").forEach { forbidden -> assertFalse(lines.any { forbidden in it }) }
        } finally { log.close() }
    }

    @Test fun aFailingDetailProviderCannotEscapeTheObserver() {
        val lines = mutableListOf<String>()
        val log = ConnectionLog(clock = { 100 }, write = { _, line -> lines += line })
        try {
            val observer = CarPlayDiagnostics(secrets = { error("provider unavailable") }, timeline = log)
            observer.onLinkEnded(CarPlayLinkEnd("bluetooth", "connecting", "local", "connect_failed", "IOException"))
            log.awaitIdle()
            assertTrue(lines.isEmpty())
        } finally { log.close() }
    }

    @Test fun usbRunDecisionFieldsSurviveTheObserverFilterAfterFailed() {
        val lines = mutableListOf<String>()
        val log = ConnectionLog(clock = { 100 }, write = { _, line -> lines += line })
        try {
            val observer = CarPlayDiagnostics(timeline = log)
            observer.onStatus(CarPlayStatus.Failed("USB connection failed"))
            observer.onLinkEnded(CarPlayLinkEnd("usb", "connecting_control", "local", "connect_failed",
                facts = mapOf("next" to "rebuild", "nextInMs" to "1500")))
            log.awaitIdle()
            val interrupted = lines.single { it.startsWith("phone interrupted ") }
            assertTrue(interrupted.contains("link=usb by=local cause=connect_failed next=rebuild nextInMs=1500"))
            assertTrue(lines.any { it.startsWith("phone link_ended ") && "next=rebuild nextInMs=1500" in it })
        } finally { log.close() }
    }
}
