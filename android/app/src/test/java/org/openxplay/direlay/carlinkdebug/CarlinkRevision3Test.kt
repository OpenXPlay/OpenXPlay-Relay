// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-23
package org.openxplay.direlay.carlinkdebug

import android.app.Application
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.openxplay.direlay.logs.*
import org.openxplay.stream.clink.trace.*
import java.nio.file.Files

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [33], manifest = Config.NONE, application = Application::class)
class CarlinkRevision3Test {
    @Test fun carIdentityGeometryAndLaterChangesAreSummarizedOnce() {
        val lines = mutableListOf<String>()
        val timeline = mutableListOf<String>()
        val sink = CarlinkSummaryTrace({ lines += it }, timeline = { timeline += it }, car = { timeline += "car $it" })
        try {
            fun link(stage: String, fields: Map<String, String> = emptyMap()) = sink.record(ClinkTraceEvent.Link(0, 1, stage, fields))
            link("link_start"); link("ble_advertisement", mapOf("name" to "My Car"))
            link("ble_reply_name", mapOf("name" to "Head Unit"))
            link("head_unit_certificate", mapOf("vendor_name" to "Vendor", "product_name" to "Product"))
            link("wifi_handoff", mapOf("ssid" to "Car WiFi")); link("ble_exchange_done", mapOf("detail" to "ble_exchange_done frequency=5180"))
            sink.record(ClinkTraceEvent.Negotiation(0, 1, "1280x720", listOf("avc1.640020"), true, 1234, "display"))
            sink.record(ClinkTraceEvent.Rtsp(0, 1, "RX", "RTSP/1.0 200 OK", emptyMap(), "iccoa_support_hevc: 1\nwfd_uibc_capability: GENERIC"))
            sink.record(ClinkTraceEvent.Frame(0, 1, "RX", 1, "control", 1, 25, 2, 1, 0,
                mapOf(2 to ClinkTraceField.Number(1532uL), 3 to ClinkTraceField.Number(1013uL))))
            sink.view(1156, 764); link("state", mapOf("detail" to "state=PLAYING"))
            sink.video("H264", 1280, 720, 1); sink.video("H264", 1280, 720, 2)
            sink.view(1200, 800)
            sink.awaitIdle()
            val summary = lines.single { it.startsWith("carlink car ") }
            for (fact in listOf("vendor=\"Vendor\"", "product=\"Product\"", "name=\"Head Unit\"", "ssid=\"Car WiFi\"",
                "frequency=\"5180\"", "view=\"1156x764\"", "window=\"1532x1013\"", "stream=\"1280x720\"", "codec=\"H264\"", "hevc=\"1\"", "audio=\"true\"", "uibc=\"GENERIC\"")) assertTrue(fact, summary.contains(fact))
            assertEquals(1, timeline.count { it.startsWith("first_video ") })
            assertTrue(timeline.any { it == "view_changed view=1200x800" })
        } finally { sink.close() }
    }
    @Test fun developerMirrorKeepsLongLinesAndRedactsBeforeSplitting() {
        val directory = Files.createTempDirectory("trace-full").toFile()
        val lines = mutableListOf<String>()
        val redactor = LogRedactor(LogSecrets(key = ByteArray(32)))
        val recorder = CarlinkTraceRecorder(directory, { lines += it }, clean = { TracePrivacy.redact(it, redactor) })
        try {
            val body = "wfd_video_formats: " + "00 ".repeat(2000) + "\npassword: top-secret-value\nwfd_audio_codecs: AAC 00000001"
            recorder.record(ClinkTraceEvent.Rtsp(1, 1, "RX", "RTSP/1.0 200 OK", mapOf("Authorization" to "Bearer credential-value"), body))
            recorder.awaitIdle()
            assertTrue(lines.size > 1)
            assertTrue(lines.all { it.toByteArray().size <= 3500 })
            val reconstructed = lines.joinToString("") { it.substringBeforeLast(" part=") }
            assertTrue(reconstructed.contains("wfd_video_formats")); assertTrue(reconstructed.contains("wfd_audio_codecs"))
            assertFalse(reconstructed.contains("top-secret-value")); assertFalse(reconstructed.contains("credential-value"))
            assertTrue(reconstructed.contains("[secret#"))
            assertEquals(reconstructed, recorder.currentFile!!.readText().trimEnd())
        } finally { recorder.close(); directory.deleteRecursively() }
    }
    @Test fun rtspPrivacyKeepsWfdCapabilitiesAndIsIdempotent() {
        val redactor = LogRedactor(LogSecrets(key = ByteArray(32)))
        val json = CarlinkTraceRecorder.encode(ClinkTraceEvent.Rtsp(1, 1, "RX", "RTSP/1.0 200 OK", emptyMap(),
            "wfd_video_formats: 00 00 02\r\nwfd_audio_codecs: AAC 00000001\r\niccoa_support_hevc: 1\r\npsk: private-car-password\r\npin: 1234"))
        val clean = TracePrivacy.redact(json, redactor)
        assertTrue(clean.contains("wfd_video_formats: 00 00 02")); assertTrue(clean.contains("iccoa_support_hevc: 1"))
        assertFalse(clean.contains("private-car-password")); assertFalse(clean.contains("1234"))
        assertEquals(clean, TracePrivacy.redact(clean, redactor))
    }
}
