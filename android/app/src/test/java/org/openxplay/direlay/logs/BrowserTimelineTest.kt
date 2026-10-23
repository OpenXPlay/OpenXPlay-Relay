// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-23
package org.openxplay.direlay.logs

import android.app.Application
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [33], manifest = Config.NONE, application = Application::class)
class BrowserTimelineTest {
    @Test fun reportsAreDeduplicatedAndNeverInventSocketLifecycle() {
        val lines = mutableListOf<String>()
        val log = ConnectionLog(clock = { 100 }, write = { _, line -> lines += line })
        val browser = BrowserTimeline(log)
        try {
            val info = """{"userAgent":"Chrome/123","screen":{"width":1280,"height":720},"videoDecoder":true,"videoDecode":[{"codec":"avc1.640020","width":1280,"height":720,"preferHardware":true}]}"""
            browser.report("car", "receiver-info", info); browser.report("car", "receiver-info", info)
            browser.report("car", "stats", """{"rendered":1}""")
            browser.report("car", "stats", """{"rendered":2}""")
            browser.report("car", "relay-close", "write_timeout")
            browser.awaitIdle(); log.awaitIdle()
            assertEquals(1, lines.count { it.startsWith("browser client_observed ") })
            assertEquals(1, lines.count { it.startsWith("browser receiver_info ") })
            assertEquals(1, lines.count { it.startsWith("browser first_render_observed ") })
            assertEquals(1, lines.count { it.startsWith("browser client_disconnected ") })
            assertTrue(lines.any { it.contains("ua=Chrome screen=1280x720") && it.contains("avc1.640020") })
            assertFalse(lines.any { it.startsWith("browser client_connected ") || it.contains("first_frame_delivered") })
        } finally { browser.close(); log.close() }
    }
}
