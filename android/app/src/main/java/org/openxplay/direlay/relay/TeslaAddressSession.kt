// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-22
package org.openxplay.direlay.relay

import org.openxplay.direlay.network.TeslaAddressVpn
import org.openxplay.direlay.settings.*

data class TeslaAddress(val mode: TeslaBrowserMode, val address: String)

/** Captures next-start choices and isolates VPN failure from the CarPlay session. */
class TeslaAddressSession(private val vpn: TeslaAddressVpn, private val clock: () -> Long = System::nanoTime) : AutoCloseable {
    @Volatile var active: TeslaAddress? = null
        private set
    @Volatile var failure: String? = null
        private set
    private var prepared: Pair<RelaySettings, Boolean>? = null
    private var session: Pair<RelaySettings, Boolean>? = null
    private var running = false
    private var disabled = false
    private var attempted = false
    private var request: Long? = null
    private var deadline = 0L

    @Synchronized fun prepare(settings: RelaySettings, wireless: Boolean) {
        stop()
        running = false
        session = null
        prepared = settings to wireless
    }

    @Synchronized fun tick(isRunning: Boolean, settings: RelaySettings, wireless: Boolean,
        trustedNameConfigured: Boolean, phoneAddresses: () -> Set<String>) {
        if (!isRunning) {
            if (running) { stop(); session = null }
            running = false
            return
        }
        if (!running) {
            session = prepared ?: (settings to wireless)
            prepared = null
            disabled = false; attempted = false; failure = null
            running = true
        }
        if (settings.teslaBrowser == TeslaBrowserMode.OFF) { stop(); return }
        if (disabled) return
        val (chosen, useWireless) = checkNotNull(session)
        if (teslaBrowserActivation(chosen.teslaBrowser, useWireless, chosen.wirelessNetwork.choice,
                trustedNameConfigured, chosen.carlinkOutput, chosen.bothOutputs) != TeslaBrowserActivation.READY) {
            if (request != null) stop()
            return
        }
        if (!attempted) {
            attempted = true
            try {
                val address = checkNotNull(chosen.teslaAddress())
                require(TeslaAddressPolicy.valid(address, phoneAddresses())) { "Invalid or already assigned Tesla address: $address" }
                request = vpn.start(address)
                deadline = clock() + 15_000_000_000L
            } catch (cause: Exception) { failure = failureText(cause) { it } }
        }
        val id = request ?: return
        val state = vpn.snapshot()
        when {
            state.request == id && state.active -> active = TeslaAddress(chosen.teslaBrowser, checkNotNull(state.address))
            state.request == id && state.failure != null -> {
                val cause = state.failure
                stop()
                failure = cause
            }
            clock() >= deadline -> {
                stop()
                failure = "Timed out waiting for the Tesla address VPN"
            }
        }
    }

    /** Off and Stop remove the address immediately, even if the input service is still winding down. */
    @Synchronized fun stop() {
        prepared = null
        disabled = true
        active = null
        failure = null
        if (request != null) {
            request = null
            try { vpn.stop() } catch (cause: Exception) { failure = failureText(cause) { it } }
        }
    }

    @Synchronized override fun close() { stop(); prepared = null; session = null; running = false }
}
