// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-16
package org.openxplay.direlay.admin

import android.app.Application
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import org.openxplay.direlay.path.PathInput
import org.openxplay.stream.core.VideoCodec
import org.openxplay.stream.core.VideoCodecDecision
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [28], manifest = Config.NONE, application = Application::class)
class AdminStatusJsonTest {
    private val base = AdminStatus("android", "OpenXPlay Relay", "test", "openxplay-android.local", "https://openxplay-android.local:6975/",
        emptyList(), "active", true, false, AdminIphone("iPhone", "connected"), AdminNetwork("wifi-direct", null), "web", null, emptyList())

    @Test fun statusIncludesTheAndroidPathInputAlongsideExistingFields() {
        val input = PathInput(nowMillis = 1793000000100, version = "0.2.1")
        val status = JSONObject(base.copy(pathInput = input).json().toString())
        val path = status.getJSONObject("pathInput")
        assertEquals("android", path.getString("platform"))
        assertEquals(1793000000100, path.getLong("nowMillis"))
        assertEquals("0.2.1", path.getString("version"))
        assertEquals(readJsonValue(input.json()), readJsonValue(path))
        assertEquals("iPhone", status.getJSONObject("iphone").getString("name"))
        assertEquals("web", status.getString("output"))
        assertEquals("android", base.json().getJSONObject("pathInput").getString("platform"))
    }

    @Test fun pairingWindowAndPairedDevicesUseTheSharedContract() {
        val json = base.copy(pairingAvailable = true, pairingDeadline = 1_600, pairingRemaining = 581,
            pairedDevices = listOf(AdminPairedDevice("Riko's iPhone", true, true), AdminPairedDevice("", false, false))).json()
        assertTrue(json.getBoolean("pairingAvailable"))
        assertEquals(1_600, json.getLong("pairingDeadline"))
        assertEquals(581, json.getLong("pairingRemaining"))
        assertFalse(json.getBoolean("pairingCancelable"))
        assertFalse(json.getBoolean("pairingNeedsConfirmation"))
        assertFalse(json.getBoolean("pairingOnRelay"))
        assertFalse(json.getBoolean("pairingNeedsPermission"))
        assertFalse(json.getBoolean("pairingPermissionDenied"))
        assertFalse(json.getBoolean("pairingNotified"))
        val device = json.getJSONArray("pairedDevices").getJSONObject(0)
        assertEquals("Riko's iPhone", device.getString("name"))
        assertTrue(device.getBoolean("connected") && device.getBoolean("iphone"))
        assertEquals("", json.getJSONArray("pairedDevices").getJSONObject(1).getString("name"))
        val closed = base.copy(pairingNeedsConfirmation = true).json()
        assertTrue(closed.isNull("pairingDeadline") && closed.isNull("pairingRemaining"))
        assertTrue(closed.getBoolean("pairingNeedsConfirmation"))
        assertEquals(0, closed.getJSONArray("pairedDevices").length())
        assertFalse(closed.has("receivers"))
    }

    @Test fun nearbyDevicesPermissionStepsReachTheAdminPage() {
        val waiting = base.copy(pairingNeedsPermission = true, pairingNotified = true).json()
        assertTrue(waiting.getBoolean("pairingNeedsPermission") && waiting.getBoolean("pairingNotified"))
        assertTrue(waiting.isNull("pairingDeadline"))
        val denied = base.copy(pairingOnRelay = true, pairingPermissionDenied = true).json()
        assertTrue(denied.getBoolean("pairingOnRelay") && denied.getBoolean("pairingPermissionDenied"))
        assertFalse(denied.getBoolean("pairingNeedsPermission"))
    }

    @Test fun videoStatusCarriesTheStreamFormatAndPendingChanges() {
        val hevc = VideoCodecDecision(VideoCodec.H265, "all-main-screen-outputs-hardware-hevc")
        val json = base.copy(video = videoStatus("H265", hevc, "hvc1.1.6.L120.B0", 1920, 720)).json().getJSONObject("video")
        assertEquals("H265", json.getString("codec"))
        assertEquals("all-main-screen-outputs-hardware-hevc", json.getString("reason"))
        assertEquals("hvc1.1.6.L120.B0", json.getString("codecString"))
        assertEquals(1920, json.getInt("width")); assertEquals(720, json.getInt("height"))
        val pending = base.copy(video = videoStatus("H264", hevc, null, null, null)).json().getJSONObject("video")
        assertEquals("all-main-screen-outputs-hardware-hevc; codec-change-pending", pending.getString("reason"))
        assertTrue(pending.isNull("codecString") && pending.isNull("width") && pending.isNull("height"))
    }

    @Test fun receiversDropTheMessageTypeAndSkipMalformedReports() {
        val receiver = AdminReceiver("3", 1.5, null, listOf("""{"type":"stats","renderedFps":30}""", "not json", "[1]")).json()
        assertEquals("3", receiver.getString("client"))
        assertEquals(1.5, receiver.getDouble("idleSeconds"), 0.0)
        assertTrue(receiver.isNull("info"))
        assertEquals(1, receiver.getJSONArray("stats").length())
        assertEquals("""{"renderedFps":30}""", receiver.getJSONArray("stats").getJSONObject(0).toString())
        val status = base.copy(receivers = listOf(AdminReceiver("3", 0.0, """{"type":"receiver-info","refreshHz":60}""", emptyList()))).json()
        assertEquals(60, (status.getJSONArray("receivers").get(0) as JSONObject).getJSONObject("info").getInt("refreshHz"))
    }
}
