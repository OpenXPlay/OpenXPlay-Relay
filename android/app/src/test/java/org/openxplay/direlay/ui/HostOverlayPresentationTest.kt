// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-15
package org.openxplay.direlay.ui

import com.shilapi.xcertplay.orchestration.MfiTarget
import org.junit.Assert.*
import org.junit.Test
import org.openxplay.direlay.R
import org.openxplay.direlay.settings.ConnectionMode
import org.openxplay.direlay.settings.RelaySettings
import org.openxplay.stream.core.SessionPhase

class HostOverlayPresentationTest {
    private val settings = RelaySettings(ConnectionMode.USB, MfiTarget.LOCAL, "", "", "", null,
        30, false, false, false, null)
    private val home = HomeSnapshot(settings, wireless = false, permissionsReady = true, identityReady = true,
        started = true)
    private val snapshot = HostOverlaySnapshot(home, "My iPhone", listOf("https://192.168.1.2:8443/"))

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
    }

    @Test fun outputsIncludeBothWhenEnabledAndUrlsOnlyForBrowser() {
        fun output(carlink: Boolean, both: Boolean) = presentHostOverlay(snapshot.copy(
            home = home.copy(settings = settings.copy(carlinkOutput = carlink, bothOutputs = both))))
        assertEquals(listOf(R.string.ui_browser), output(false, false).outputs)
        assertEquals(snapshot.receiverUrls, output(false, false).receiverUrls)
        assertEquals(listOf(R.string.ui_carlink), output(true, false).outputs)
        assertTrue(output(true, false).receiverUrls.isEmpty())
        for (carlink in listOf(false, true)) {
            assertEquals(listOf(R.string.ui_browser, R.string.ui_carlink), output(carlink, true).outputs)
            assertEquals(snapshot.receiverUrls, output(carlink, true).receiverUrls)
        }
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
    @Test fun landingAddressUsesTheFirstReachableTlsHostWithoutItsPort() {
        assertEquals("192.168.1.2", landingHost(listOf("https://192.168.1.2:8443/", "https://10.0.0.2:8443/")))
        assertEquals("[2001:db8::1]", landingHost(listOf("https://[2001:db8::1]:8443/")))
        assertNull(landingHost(emptyList()))
        assertNull(landingHost(listOf("http://192.168.1.2:8080/")))
        assertNull(landingHost(listOf("bad address")))
    }

}
