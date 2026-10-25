// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-22
package org.openxplay.direlay.relay

import org.openxplay.stream.core.FrameRatePreference
import com.shilapi.xcertplay.orchestration.MfiTarget
import org.junit.Assert.*
import org.junit.Test
import org.openxplay.direlay.network.TeslaAddressVpn
import org.openxplay.direlay.network.TeslaVpnStatus
import org.openxplay.direlay.settings.*

class TeslaAddressSessionTest {
    private val settings = RelaySettings(ConnectionMode.WIRELESS, MfiTarget.LOCAL, "", "", "", null,
        FrameRatePreference.AUTO, false, true, false, null, teslaBrowser = TeslaBrowserMode.PUBLIC_ADDRESS)
    private class Vpn : TeslaAddressVpn {
        val started = mutableListOf<String>()
        var stopped = 0
        var ready = true
        var state = TeslaVpnStatus()
        var failure: Exception? = null
        override fun start(address: String): Long {
            started.add(address)
            failure?.let { throw it }
            val id = started.size.toLong()
            state = TeslaVpnStatus(id, address, active = ready)
            return id
        }
        override fun stop() { stopped++; state = TeslaVpnStatus() }
        override fun snapshot() = state
    }
    private val vpn = Vpn()
    private var now = 0L
    private val session = TeslaAddressSession(vpn) { now }
    private fun tick(running: Boolean = true, value: RelaySettings = settings, wireless: Boolean = true,
        name: Boolean = true, addresses: Set<String> = emptySet()) = session.tick(running, value, wireless, name) { addresses }

    @Test fun startsOnlyWithTheSessionStopsWithItAndClosesCleanly() {
        tick(false)
        assertTrue(vpn.started.isEmpty())
        tick(); tick()
        assertEquals(listOf("169.254.88.1"), vpn.started)
        assertEquals(TeslaAddress(TeslaBrowserMode.PUBLIC_ADDRESS, "169.254.88.1"), session.active)
        tick(false)
        assertNull(session.active); assertEquals(1, vpn.stopped)
        tick(); session.close()
        assertEquals(2, vpn.stopped); assertNull(session.active)
    }

    @Test fun liveModeAndAddressChangesKeepTheCapturedTransportNetworkAndOutputs() {
        session.prepare(settings, wireless = true)
        tick()
        for (mode in TeslaBrowserMode.entries.filter { it != TeslaBrowserMode.OFF }) {
            val next = settings.copy(teslaBrowser = mode, connectionMode = ConnectionMode.USB,
                wirelessNetwork = WirelessNetworkSettings(choice = WirelessNetworkChoice.EXISTING_WIFI),
                carlinkOutput = true, bothOutputs = false)
            val starts = vpn.started.size
            val stops = vpn.stopped
            tick(value = next, wireless = false)
            assertEquals(TeslaAddress(mode, checkNotNull(next.teslaAddress())), session.active)
            assertEquals(starts + 1, vpn.started.size)
            assertEquals(stops + 1, vpn.stopped)
            tick(value = next, wireless = false)
            assertEquals(starts + 1, vpn.started.size)
        }
    }

    @Test fun switchingModesWithTheSameAddressIgnoresTheTunnelBeingRemoved() {
        tick()
        val named = settings.copy(teslaBrowser = TeslaBrowserMode.VIRTUAL_NAME)
        tick(value = named, addresses = setOf("169.254.88.1"))
        assertNull(session.failure)
        assertEquals(TeslaAddress(TeslaBrowserMode.VIRTUAL_NAME, "169.254.88.1"), session.active)
        assertEquals(listOf("169.254.88.1", "169.254.88.1"), vpn.started)
        assertEquals(1, vpn.stopped)
    }

    @Test fun editingTheSelectedAddressReestablishesTheVpnInTheSameSession() {
        tick()
        val edited = settings.copy(teslaPublicAddress = "169.254.88.2")
        tick(value = edited); tick(value = edited)
        assertEquals(listOf("169.254.88.1", "169.254.88.2"), vpn.started)
        assertEquals(1, vpn.stopped)
        assertEquals(TeslaAddress(TeslaBrowserMode.PUBLIC_ADDRESS, "169.254.88.2"), session.active)
        tick(value = edited.copy(teslaMirrorAddress = "100.88.0.2"))
        assertEquals(2, vpn.started.size)
    }

    @Test fun offStopsImmediatelyAndTurningItBackOnStartsInTheSameSession() {
        tick()
        tick(value = settings.copy(teslaBrowser = TeslaBrowserMode.OFF))
        assertNull(session.active); assertEquals(1, vpn.stopped)
        tick(); tick()
        assertEquals(2, vpn.started.size)
        assertNotNull(session.active)
    }

    @Test fun aSessionStartedOffCanTurnOnAndExplicitStopStillDisablesIt() {
        val off = settings.copy(teslaBrowser = TeslaBrowserMode.OFF)
        session.prepare(off, wireless = true)
        tick(value = off)
        assertTrue(vpn.started.isEmpty())
        tick()
        assertNotNull(session.active)
        session.stop()
        tick(value = settings.copy(teslaBrowser = TeslaBrowserMode.TESLA_ANDROID))
        assertNull(session.active)
        assertEquals(1, vpn.started.size)
    }

    @Test fun aFailureAfterSwitchIsReportedAndALaterSwitchRetries() {
        tick()
        vpn.failure = SecurityException("VPN consent revoked")
        val mirror = settings.copy(teslaBrowser = TeslaBrowserMode.TESLAMIRROR)
        tick(value = mirror); tick(value = mirror)
        assertNull(session.active)
        assertEquals("VPN consent revoked", session.failure)
        assertEquals(2, vpn.started.size)
        vpn.failure = null
        val direct = settings.copy(teslaBrowser = TeslaBrowserMode.TESLA_ANDROID)
        tick(value = direct)
        assertNull(session.failure)
        assertEquals(TeslaAddress(TeslaBrowserMode.TESLA_ANDROID, "3.3.3.1"), session.active)
        assertEquals(3, vpn.started.size)
    }

    @Test fun turningOnWithoutConsentReportsFailureUntilAnotherPairIsSelected() {
        tick(value = settings.copy(teslaBrowser = TeslaBrowserMode.OFF))
        vpn.failure = SecurityException("VPN consent required")
        tick(); tick()
        assertEquals("VPN consent required", session.failure)
        assertEquals(1, vpn.started.size)
        tick(value = settings.copy(teslaBrowser = TeslaBrowserMode.OFF))
        assertNull(session.failure)
        vpn.failure = null
        tick()
        assertNotNull(session.active)
    }

    @Test fun readsLivePhoneAddressesOnlyAtStartAndRechecksThemOnRestart() {
        var addresses = setOf("169.254.88.1")
        var reads = 0
        session.prepare(settings, wireless = true)
        addresses = emptySet()
        val source = { reads++; addresses }
        session.tick(true, settings, true, true, source)
        session.tick(true, settings, true, true, source)
        assertNotNull(session.active)
        assertEquals(1, reads)
        session.tick(false, settings, true, true, source)
        addresses = setOf("169.254.88.1")
        session.tick(true, settings, true, true, source)
        assertNull(session.active)
        assertTrue(checkNotNull(session.failure).contains("already assigned"))
        assertEquals(2, reads)
    }

    @Test fun preparingUsbRemovesTheWirelessAddressBeforeTheNextInputStarts() {
        tick()
        val usb = settings.copy(connectionMode = ConnectionMode.USB)
        session.prepare(usb, wireless = false)
        assertNull(session.active)
        assertEquals(1, vpn.stopped)
        tick(false, usb, wireless = false)
        tick(value = usb, wireless = false)
        assertEquals(1, vpn.started.size)
    }

    @Test fun conflictsNeverStartAVpnAndANameCanBeConfiguredWhileWaiting() {
        for (value in listOf(settings.copy(teslaBrowser = TeslaBrowserMode.OFF),
            settings.copy(wirelessNetwork = WirelessNetworkSettings(choice = WirelessNetworkChoice.EXISTING_WIFI)))) {
            tick(value = value); tick(false, value)
        }
        tick(wireless = false); tick(false)
        assertTrue(vpn.started.isEmpty())
        val named = settings.copy(teslaBrowser = TeslaBrowserMode.VIRTUAL_NAME)
        tick(value = named, name = false)
        assertTrue(vpn.started.isEmpty())
        tick(value = named, name = true)
        assertEquals("169.254.88.1", session.active?.address)
    }

    @Test fun carlinkOnlyNeverStartsAVpnButBrowserAndCombinedOutputsDo() {
        for (mode in TeslaBrowserMode.entries.filter { it != TeslaBrowserMode.OFF })
            for ((carlink, both, browser) in listOf(Triple(false, false, true), Triple(false, true, true),
                Triple(true, false, false), Triple(true, true, true))) {
                val chosen = settings.copy(teslaBrowser = mode, carlinkOutput = carlink, bothOutputs = both)
                val controller = Vpn()
                TeslaAddressSession(controller).use { candidate ->
                    candidate.prepare(chosen, wireless = true)
                    repeat(2) { candidate.tick(true, chosen, true, true) { emptySet() } }
                    if (browser) {
                        assertEquals(listOf(chosen.teslaAddress()), controller.started)
                        assertEquals(TeslaAddress(mode, checkNotNull(chosen.teslaAddress())), candidate.active)
                    } else {
                        assertTrue(controller.started.isEmpty())
                        assertNull(candidate.active)
                    }
                    assertNull(candidate.failure)
                }
            }
    }

    @Test fun serviceFailureKeepsTheRawCauseWithoutRetryingOrThrowingIntoCarPlay() {
        tick()
        vpn.state = vpn.state.copy(active = false, failure = "IOException: establish failed\nCaused by: OEM reason")
        tick()
        assertNull(session.active)
        assertEquals("IOException: establish failed\nCaused by: OEM reason", session.failure)
        now += 30_000_000_000L
        tick()
        assertTrue(checkNotNull(session.failure).contains("OEM reason"))
        assertEquals(1, vpn.started.size)
        tick(value = settings.copy(teslaBrowser = TeslaBrowserMode.TESLA_ANDROID))
        assertNull(session.failure)
        assertEquals(TeslaAddress(TeslaBrowserMode.TESLA_ANDROID, "3.3.3.1"), session.active)
        assertEquals(2, vpn.started.size)
        tick(false)
        assertNull(session.failure)
    }

    @Test fun startExceptionsAddressCollisionsAndTimeoutsStayLocalToTheAdaptation() {
        vpn.failure = SecurityException("VPN consent revoked")
        tick(); tick()
        assertEquals("VPN consent revoked", session.failure)
        assertEquals(1, vpn.started.size)
        tick(false)
        vpn.failure = null
        tick(addresses = setOf("169.254.88.1"))
        assertTrue(checkNotNull(session.failure).contains("already assigned"))
        assertEquals(1, vpn.started.size)
        tick(false)
        vpn.ready = false
        tick()
        assertNull(session.active)
        now += 15_000_000_000L
        tick()
        assertTrue(checkNotNull(session.failure).contains("Timed out"))
        assertEquals(1, vpn.stopped)
    }
}
