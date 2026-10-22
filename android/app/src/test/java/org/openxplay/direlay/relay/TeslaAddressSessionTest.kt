// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-22
package org.openxplay.direlay.relay

import com.shilapi.xcertplay.orchestration.MfiTarget
import org.junit.Assert.*
import org.junit.Test
import org.openxplay.direlay.network.TeslaAddressVpn
import org.openxplay.direlay.network.TeslaVpnStatus
import org.openxplay.direlay.settings.*

class TeslaAddressSessionTest {
    private val settings = RelaySettings(ConnectionMode.WIRELESS, MfiTarget.LOCAL, "", "", "", null,
        30, false, true, false, null, teslaBrowser = TeslaBrowserMode.PUBLIC_ADDRESS)
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
        assertEquals(listOf("3.3.3.1"), vpn.started)
        assertEquals(TeslaAddress(TeslaBrowserMode.PUBLIC_ADDRESS, "3.3.3.1"), session.active)
        tick(false)
        assertNull(session.active); assertEquals(1, vpn.stopped)
        tick(); session.close()
        assertEquals(2, vpn.stopped); assertNull(session.active)
    }

    @Test fun capturesModeAddressAndActualTransportAtTheNextStart() {
        session.prepare(settings, wireless = true)
        val next = settings.copy(teslaBrowser = TeslaBrowserMode.VIRTUAL_NAME, teslaVirtualAddress = "100.88.0.2")
        tick(value = next, wireless = false)
        assertEquals("3.3.3.1", session.active?.address)
        tick(value = next)
        assertEquals(listOf("3.3.3.1"), vpn.started)
        tick(false, next)
        session.prepare(next, wireless = true)
        tick(value = next)
        assertEquals("100.88.0.2", session.active?.address)
        assertEquals(TeslaBrowserMode.VIRTUAL_NAME, session.active?.mode)
    }

    @Test fun offStopsImmediatelyAndTurningItBackOnWaitsForAnotherStart() {
        tick()
        tick(value = settings.copy(teslaBrowser = TeslaBrowserMode.OFF))
        assertNull(session.active); assertEquals(1, vpn.stopped)
        tick()
        assertEquals(1, vpn.started.size)
        tick(false); tick()
        assertEquals(2, vpn.started.size)
    }

    @Test fun readsLivePhoneAddressesOnlyAtStartAndRechecksThemOnRestart() {
        var addresses = setOf("3.3.3.1")
        var reads = 0
        session.prepare(settings, wireless = true)
        addresses = emptySet()
        val source = { reads++; addresses }
        session.tick(true, settings, true, true, source)
        session.tick(true, settings, true, true, source)
        assertNotNull(session.active)
        assertEquals(1, reads)
        session.tick(false, settings, true, true, source)
        addresses = setOf("3.3.3.1")
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
        assertEquals("100.88.0.1", session.active?.address)
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
        tick(addresses = setOf("3.3.3.1"))
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
