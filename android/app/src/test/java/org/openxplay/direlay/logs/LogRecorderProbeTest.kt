// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-11-01
package org.openxplay.direlay.logs

import android.app.Application
import android.util.Log
import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.shadows.ShadowLog

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [33], manifest = Config.NONE, application = Application::class)
class LogRecorderProbeTest {
    @get:Rule val temporary = TemporaryFolder()

    @Test fun recorderStartEmitsOnePersonalDataFreeMarkerPerPriority() {
        val recorder = LogRecorder(LogSegmentStore(temporary.newFolder()), LogRedactor())
        try {
            recorder.start("Test", 1)
            recorder.start("Test", 1)
            val markers = ShadowLog.getLogsForTag(LogcatProbe.TAG)
            assertEquals(listOf(Log.VERBOSE, Log.DEBUG, Log.INFO, Log.WARN, Log.ERROR), markers.map { it.type })
            val nonces = markers.map { it.msg.substringAfter("nonce=").substringBefore(' ') }.toSet()
            assertEquals(1, nonces.size)
            java.util.UUID.fromString(nonces.single())
            markers.zip(listOf('V', 'D', 'I', 'W', 'E')).forEach { (marker, priority) ->
                assertEquals("nonce=${nonces.single()} priority=$priority", marker.msg)
            }
        } finally { recorder.close() }
    }
    @Test fun storageLossIsCountedAfterTheRawProbeBoundary() {
        val probe = LogcatProbe({ 0L }, { "nonce" })
        val recorder = LogRecorder(LogSegmentStore(temporary.newFile()), LogRedactor(), clock = { 0 }, probe = probe)
        try {
            probe.start { priority, marker ->
                recorder.capture("2026-11-01 05:05:00.000 +0000 1 2 $priority ${LogcatProbe.TAG}: $marker")
                10
            }
            assertEquals(setOf('V', 'D', 'I', 'W', 'E'), probe.result().seen)
            assertEquals(5L, recorder.captureFailures)
        } finally { recorder.close() }
    }

}
