// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-15
package org.openxplay.direlay.ui

import com.shilapi.xcertplay.orchestration.MfiTarget
import org.junit.Assert.*
import org.junit.Test
import org.openxplay.direlay.R
import org.openxplay.direlay.relay.WebStartReason
import org.openxplay.direlay.relay.WebStartFailure
import org.openxplay.direlay.relay.RelayFailure
import org.openxplay.direlay.relay.VideoMeasurement
import org.openxplay.direlay.relay.VideoStatsSnapshot
import org.openxplay.direlay.settings.CarPlayScreenWidth
import org.openxplay.direlay.settings.ConnectionMode
import org.openxplay.direlay.settings.RelaySettings
import org.openxplay.direlay.settings.VideoStatsWindow
import org.openxplay.stream.core.SessionPhase
import org.openxplay.stream.core.VideoCodec
import java.util.Locale

class HostOverlayPresentationTest {
    private val settings = RelaySettings(ConnectionMode.USB, MfiTarget.LOCAL, "", "", "", null,
        30, false, false, false, null)
    private val home = HomeSnapshot(settings, wireless = false, permissionsReady = true, identityReady = true,
        started = true)
    private val snapshot = HostOverlaySnapshot(home, "My iPhone", listOf("https://192.168.1.2:8443/"))

    @Test fun webStartReasonsKeepErrorHeadlineAndPortArgument() {
        for (reason in WebStartFailure.entries) {
            val result = presentHostOverlay(snapshot.copy(receiverUrls = emptyList(), home = home.copy(
                phase = SessionPhase.ACTIVE, webReason = WebStartReason(reason, 9443))))
            assertEquals(R.string.ui_error, result.headline)
            assertEquals(StatusTone.ERROR, result.tone)
            assertEquals(if (reason == WebStartFailure.PORT_BUSY) R.string.ui_web_port_busy else R.string.ui_web_start_failed,
                result.hint)
            assertEquals(if (reason == WebStartFailure.PORT_BUSY) listOf("9443") else emptyList<String>(), result.hintArguments)
            assertNull(result.browserHint)
            assertNull(result.iphoneName)
            assertEquals(presentHome(home.copy(webReason = WebStartReason(reason, 9443))).problems, result.problems)
        }
    }

    @Test fun listeningWithoutCarFacingAddressShowsNetworkInstructions() {
        val result = presentHostOverlay(snapshot.copy(receiverUrls = emptyList(), home = home.copy(
            webListening = true, carNetworkAvailable = false)))
        assertEquals(R.string.ui_head_unit_network, result.browserHint)
        assertNotEquals(StatusTone.ERROR, result.tone)
        assertEquals(R.string.ui_browser_https_hint, presentHostOverlay(snapshot).browserHint)
    }

    @Test fun waitingAndConnectingOfferCancellationAndKeepThePhoneName() {
        for (phase in listOf(null, SessionPhase.CONNECTING)) {
            val result = presentHostOverlay(snapshot.copy(home = home.copy(phase = phase)))
            assertEquals(R.string.host_connecting, result.headline)
            assertEquals(StatusTone.PROGRESS, result.tone)
            assertEquals("My iPhone", result.iphoneName)
            assertEquals(HostOverlayButton(R.string.host_cancel, HostOverlayAction.STOP), result.buttons.last())
            assertEquals(HostOverlayButton(R.string.host_back, HostOverlayAction.BACK), result.buttons.first())
        }
        assertNull(presentHostOverlay(snapshot.copy(iphoneName = " ")).iphoneName)
    }

    @Test fun runningExplainsWhereCarPlayIsAndHowToKeepItRunning() {
        val result = presentHostOverlay(snapshot.copy(home = home.copy(phase = SessionPhase.ACTIVE)))
        assertEquals(StatusTone.RUNNING, result.tone)
        assertEquals(R.string.host_running, result.headline)
        assertEquals(R.string.host_keep_running, result.hint)
        assertEquals(HostOverlayButton(R.string.ui_stop, HostOverlayAction.STOP), result.buttons.last())
    }

    @Test fun reconnectAndErrorsReuseHomeCopyAndNeverClaimSuccess() {
        for (phase in listOf(SessionPhase.RECONNECTING, SessionPhase.ENDED)) {
            val result = presentHostOverlay(snapshot.copy(home = home.copy(phase = phase)))
            assertEquals(R.string.ui_connecting, result.headline)
            assertEquals(R.string.ui_connecting_hint, result.hint)
            assertEquals(StatusTone.PROGRESS, result.tone)
        }
        val failed = home.copy(phase = SessionPhase.ACTIVE, failure = true)
        val result = presentHostOverlay(snapshot.copy(home = failed))
        assertEquals(presentHome(failed).headline, result.headline)
        assertEquals(presentHome(failed).subline, result.hint)
        assertEquals(StatusTone.ERROR, result.tone)
        assertEquals(R.string.ui_stop, result.buttons.last().label)
        assertNull(result.iphoneName)
        assertEquals(presentHome(failed).problems, result.problems)
    }

    @Test fun outputsIncludeBothWhenEnabledAndUrlsOnlyForBrowser() {
        fun output(carlink: Boolean, both: Boolean) = presentHostOverlay(snapshot.copy(
            home = home.copy(settings = settings.copy(carlinkOutput = carlink, bothOutputs = both))))
        assertEquals(listOf(R.string.ui_browser), output(false, false).outputs)
        assertEquals(snapshot.receiverUrls, output(false, false).receiverUrls)
        assertEquals(listOf(R.string.host_head_unit_address, R.string.picture_title, R.string.host_detected_screen),
            output(false, false).sections.map { it.title })
        assertEquals(listOf(R.string.ui_carlink), output(true, false).outputs)
        assertTrue(output(true, false).receiverUrls.isEmpty())
        assertNull(output(true, false).browserHint)
        assertEquals(listOf(R.string.ui_carlink, R.string.picture_title, R.string.host_detected_screen),
            output(true, false).sections.map { it.title })
        for (carlink in listOf(false, true)) {
            assertEquals(listOf(R.string.ui_browser, R.string.ui_carlink), output(carlink, true).outputs)
            assertEquals(snapshot.receiverUrls, output(carlink, true).receiverUrls)
            assertEquals(listOf(R.string.host_head_unit_address, R.string.ui_carlink, R.string.picture_title, R.string.host_detected_screen),
                output(carlink, true).sections.map { it.title })
        }
    }

    @Test fun trustedAddressComesFirstAndTheIpAddressesRemainAsFallbacks() {
        val trusted = "https://car.openxplay.org/"
        val ips = listOf("https://192.168.1.2:8443/", "https://10.0.0.2:8443/")
        val result = presentHostOverlay(snapshot.copy(publicNameUrl = trusted, receiverUrls = ips + trusted))
        assertEquals(listOf(trusted) + ips, result.receiverUrls)
        assertEquals(R.string.ui_browser_https_hint, result.browserHint)
        assertEquals(ips, presentHostOverlay(snapshot.copy(publicNameUrl = null, receiverUrls = ips)).receiverUrls)
        assertEquals(listOf(trusted), presentHostOverlay(snapshot.copy(publicNameUrl = trusted, receiverUrls = emptyList())).receiverUrls)
        val carlinkOnly = presentHostOverlay(snapshot.copy(publicNameUrl = trusted, home = home.copy(
            settings = settings.copy(carlinkOutput = true))))
        assertTrue(carlinkOnly.receiverUrls.isEmpty())
    }

    @Test fun carNetworkIpsStayVisibleBeforeThereIsAReceiverUrl() {
        val addresses = listOf("192.168.1.2", "10.0.0.2")
        val result = presentHostOverlay(snapshot.copy(receiverUrls = emptyList(), carIpv4 = addresses))
        assertEquals(listOf(HostOverlayDetail(R.string.network_addresses, addresses.joinToString("\n"), monospace = true)),
            result.sections.single { it.title == R.string.host_head_unit_address }.rows)
        assertTrue(presentHostOverlay(snapshot.copy(carIpv4 = addresses)).sections
            .single { it.title == R.string.host_head_unit_address }.rows.isEmpty())
    }

    @Test fun errorReusesEveryHomeProblemAndRecoveryRestoresThePhoneLine() {
        val reason = WebStartReason(WebStartFailure.PORT_BUSY, 8443)
        val problems = homeProblems(listOf(RelayFailure(null, "CarPlay failed\nCaused by: socket closed"),
            RelayFailure("web", "web output failed")), true, "auth_rejected", "Carlink listener failed",
            reason, "TLS bind failed")
        val failed = home.copy(failure = true, webReason = reason, problems = problems,
            settings = settings.copy(bothOutputs = true))
        val result = presentHostOverlay(snapshot.copy(home = failed))
        assertNull(result.iphoneName)
        assertEquals(presentHome(failed).problems, result.problems)
        assertEquals(problems, result.problems)
        val recovered = presentHostOverlay(snapshot.copy(home = failed.copy(failure = false, webReason = null,
            phase = SessionPhase.ACTIVE)))
        assertTrue(recovered.problems.isEmpty())
        assertEquals("My iPhone", recovered.iphoneName)
    }

    @Test fun pictureShowsAppliedCanvasAndPhysicalWidthAlongsideConfiguredFrameRateAndActualCodec() {
        val result = presentHostOverlay(snapshot.copy(canvas = "2560 × 1080", screenWidthMm = 275,
            videoCodec = VideoCodec.H265, home = home.copy(settings = settings.copy(carplayScreenWidth = CarPlayScreenWidth.MM125))))
        val rows = result.sections.single { it.title == R.string.picture_title }.rows
        assertEquals(HostOverlayDetail(R.string.picture_resolution, "2560 × 1080", monospace = true), rows[0])
        assertEquals(HostOverlayDetail(R.string.picture_screen_width, "275 mm", monospace = true), rows[1])
        assertEquals(HostOverlayDetail(R.string.direlay_fps, valueResource = R.string.host_frame_rate, arguments = listOf("30")), rows[2])
        assertEquals(HostOverlayDetail(R.string.picture_video_codec, "H.265"), rows[3])
        assertEquals("H.264", presentHostOverlay(snapshot.copy(videoCodec = VideoCodec.H264)).sections
            .single { it.title == R.string.picture_title }.rows.last().value)
    }

    @Test fun detectedScreenKeepsEveryLocalizedSourceAndTheViewportAspectRatio() {
        val sources = mapOf("car screen" to R.string.dev_screen_car, "remembered" to R.string.dev_screen_remembered,
            "built-in model" to R.string.dev_screen_model, "default" to R.string.dev_screen_default,
            "fixed" to R.string.dev_screen_fixed, "relay screen" to R.string.dev_screen_relay)
        val before = Locale.getDefault()
        try {
            Locale.setDefault(Locale.GERMANY)
            sources.forEach { (source, key) ->
                val result = presentHostOverlay(snapshot.copy(carPlayScreen = "1920 × 1080 · 240 dpi" to source,
                    viewport = HostOverlaySize(1280, 720)))
                val rows = result.sections.single { it.title == R.string.host_detected_screen }.rows
                assertEquals(HostOverlayDetail(R.string.dev_carplay_screen, monospace = true, valueResource = key,
                    arguments = listOf("1920 × 1080 · 240 dpi")), rows[0])
                assertEquals(HostOverlayDetail(R.string.picture_window, "1280 × 720 · 1.78:1", monospace = true), rows[1])
            }
            val unknown = presentHostOverlay(snapshot.copy(viewport = HostOverlaySize(1280, 0))).sections
                .single { it.title == R.string.host_detected_screen }.rows
            assertEquals(listOf("—", "—"), unknown.map { it.value })
        } finally { Locale.setDefault(before) }
    }

    @Test fun carlinkSectionShowsTheConnectionStateAndKnownHeadUnitWindow() {
        val carlink = snapshot.copy(home = home.copy(settings = settings.copy(carlinkOutput = true)),
            carlinkState = R.string.ui_connected, carlinkWindow = HostOverlaySize(1532, 1013))
        assertEquals(listOf(HostOverlayDetail(R.string.host_state, valueResource = R.string.ui_connected),
            HostOverlayDetail(R.string.picture_window, "1532 × 1013", monospace = true)),
            presentHostOverlay(carlink).sections.single { it.title == R.string.ui_carlink }.rows)
        assertEquals(1, presentHostOverlay(carlink.copy(carlinkWindow = null)).sections.single { it.title == R.string.ui_carlink }.rows.size)
    }

    @Test fun videoStatisticsAndMeasuredFrameRateAppearOnlyForTheEnabledWindow() {
        val measurement = VideoMeasurement.Values(29.8, 1_000_000.0, 0.0, 0, 149, 0)
        val stats = VideoStatsSnapshot(VideoStatsWindow.SECONDS_5, measurement, null, null, null, null, null, emptyList())
        val enabled = snapshot.copy(home = home.copy(settings = settings.copy(videoStats = VideoStatsWindow.SECONDS_5)), videoStats = stats)
        val result = presentHostOverlay(enabled)
        assertEquals(presentVideoStats(stats), result.videoStats)
        assertEquals(HostOverlayDetail(R.string.direlay_fps, valueResource = R.string.host_frame_rate_measured,
            arguments = listOf("30", "29.8")), result.sections.single { it.title == R.string.picture_title }.rows[2])
        val disabled = presentHostOverlay(enabled.copy(home = home))
        assertNull(disabled.videoStats)
        assertEquals(R.string.host_frame_rate, disabled.sections.single { it.title == R.string.picture_title }.rows[2].valueResource)
        for (window in listOf(VideoStatsWindow.SECONDS_5, VideoStatsWindow.SECONDS_15)) {
            val waiting = presentHostOverlay(enabled.copy(videoStats = null,
                home = home.copy(settings = settings.copy(videoStats = window))))
            assertEquals(VideoStatsText(R.string.video_stats_measuring, listOf(window.seconds.toString())), waiting.videoStats?.caption)
        }
        val changed = presentHostOverlay(enabled.copy(home = home.copy(settings = settings.copy(videoStats = VideoStatsWindow.SECONDS_15))))
        assertEquals(VideoStatsText(R.string.video_stats_measuring, listOf("15")), changed.videoStats?.caption)
        assertEquals(R.string.host_frame_rate, changed.sections.single { it.title == R.string.picture_title }.rows[2].valueResource)
    }

    @Test fun explicitStopDoesNotLookLikeReconnectionOrOfferAnotherStop() {
        val result = presentHostOverlay(snapshot.copy(home = home.copy(phase = SessionPhase.ENDED), stopped = true))
        assertEquals(R.string.host_stopped, result.headline)
        assertEquals(StatusTone.IDLE, result.tone)
        assertEquals(listOf(HostOverlayAction.BACK), result.buttons.map { it.action })
        assertNull(result.hint)
    }

    @Test fun previewDoesNotConstructAnOverlay() {
        assertNull(hostOverlayUnlessPreview(true) { error("Preview must not create UI") })
        assertEquals("overlay", hostOverlayUnlessPreview(false) { "overlay" })
    }

}
