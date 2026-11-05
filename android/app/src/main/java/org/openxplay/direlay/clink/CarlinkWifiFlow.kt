// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-11-04
package org.openxplay.direlay.clink

import java.util.concurrent.CompletableFuture

internal enum class WifiTargetMatch { SAME, DIFFERENT, UNKNOWN }
internal data class CarlinkWifiCandidate<N>(val network: N, val ssid: String?, val encrypted: Boolean?,
    val peer: String?, val local: String?, val bssid: String? = null) {
    fun match(target: String): WifiTargetMatch = when {
        ssid == null -> WifiTargetMatch.UNKNOWN
        ssid != target || encrypted == false -> WifiTargetMatch.DIFFERENT
        else -> WifiTargetMatch.SAME
    }
    val endpoints: Pair<String, String>? get() = WifiJoinPolicy.endpoints(peer, local)
}

/** One connection attempt owns its requests; borrowed system connections are never disconnected. */
internal class CarlinkWifiFlow<N>(private val ssid: String, private val driver: Driver<N>,
    private val phaseChanged: (Phase) -> Unit, private val observed: (WifiTargetMatch, Boolean) -> Unit,
    private val connectionLost: () -> Unit) : AutoCloseable {
    enum class Phase { IDENTIFYING, AUTOMATIC, MANUAL, READY, CLOSED }
    interface Observer<N> {
        fun changed(candidate: CarlinkWifiCandidate<N>)
        fun lost(network: N)
    }
    interface Driver<N> {
        fun observe(observer: Observer<N>): AutoCloseable
        fun request(observer: Observer<N>, unavailable: () -> Unit, timeoutMillis: Int): AutoCloseable
        fun later(millis: Long, action: () -> Unit): AutoCloseable
        /**
         * Asks the platform to join a saved network by itself and returns how long to wait for it
         * before requesting one; 0 when the platform needs no nudge.
         */
        fun nudgePlatformJoin(): Long = 0
    }
    val result = CompletableFuture<CarlinkWifiCandidate<N>>()
    var phase = Phase.IDENTIFYING
        private set
    private var observer: AutoCloseable? = null
    private var request: AutoCloseable? = null
    private var timer: AutoCloseable? = null
    private var claim: AutoCloseable? = null
    private var platformJoin = false
    private var generation = 0
    private var observationGeneration = 0
    private var selected: CarlinkWifiCandidate<N>? = null
    private var lossReported = false
    private val observations = mutableMapOf<N, Pair<WifiTargetMatch, Boolean>>()
    private val candidates = linkedMapOf<N, CarlinkWifiCandidate<N>>()
    private val unknownRetried = mutableSetOf<N>()
    private val automaticNetworks = mutableSetOf<N>()

    fun start() {
        check(observer == null && phase == Phase.IDENTIFYING)
        observe()
        if (phase != Phase.IDENTIFYING) return
        val nudge = driver.nudgePlatformJoin()
        timer = driver.later(IDENTIFY_MILLIS) {
            val matching = matching()
            when {
                matching.size == 1 -> select(matching.single())
                matching.isEmpty() && nudge > IDENTIFY_MILLIS -> {
                    // The platform joins the saved network within a second of the scan; adopting that
                    // link avoids the request, which is what raises MIUI's unanswerable dialog.
                    platformJoin = true
                    timer = driver.later(nudge - IDENTIFY_MILLIS) { platformJoin = false; automatic() }
                }
                else -> automatic()
            }
        }
    }
    /** Re-read identities after returning from system settings or changing location permission. */
    fun refresh() {
        if (phase != Phase.MANUAL) return
        ++observationGeneration
        observer?.close(); observer = null
        candidates.clear(); observations.clear(); automaticNetworks.clear(); unknownRetried.clear()
        observe()
    }
    private fun observe() {
        val token = ++observationGeneration
        val subscription = driver.observe(object : Observer<N> {
            override fun changed(candidate: CarlinkWifiCandidate<N>) {
                if (token == observationGeneration) seen(candidate)
            }
            override fun lost(network: N) {
                if (token != observationGeneration) return
                observations.remove(network)
                candidates.remove(network)
                automaticNetworks.remove(network)
                unknownRetried.remove(network)
                if (selected?.network == network) lost()
            }
        })
        if (phase == Phase.CLOSED) subscription.close() else observer = subscription
    }
    private fun transition(value: Phase) { phase = value; phaseChanged(value) }
    private fun seen(candidate: CarlinkWifiCandidate<N>) {
        if (phase == Phase.CLOSED) return
        candidates[candidate.network] = candidate
        val match = candidate.match(ssid)
        val state = match to (candidate.endpoints != null)
        if (observations.put(candidate.network, state) != state) observed(state.first, state.second)
        if (phase == Phase.READY) {
            val current = selected ?: return
            if (candidate.network == current.network &&
                (match == WifiTargetMatch.DIFFERENT || candidate.endpoints != current.endpoints ||
                    current.bssid != null && candidate.bssid != null && current.bssid != candidate.bssid)) lost()
            return
        }
        // A network announced during request setup may be owned by that request. Its own callback
        // must select it, otherwise cancelling the pending request could destroy a borrowed link.
        if (phase == Phase.AUTOMATIC) {
            automaticNetworks.add(candidate.network)
            if (match == WifiTargetMatch.SAME && candidate.endpoints != null) awaitClaim()
            return
        }
        if (phase == Phase.IDENTIFYING && platformJoin && match == WifiTargetMatch.SAME && candidate.endpoints != null) {
            if (matching().size == 1) select(candidate)
            return
        }
        if (phase == Phase.IDENTIFYING || candidate.endpoints == null || candidate.network in automaticNetworks) return
        if (match == WifiTargetMatch.SAME && matching().size == 1) select(candidate)
        else if (phase == Phase.MANUAL && match == WifiTargetMatch.UNKNOWN && unknownRetried.add(candidate.network)) {
            // A new system connection can make the exact-target request work even when the SSID
            // is redacted. It still has to pass that request; an unknown network is never adopted.
            automatic()
        }
    }
    private fun matching() = candidates.values.filter {
        it.network !in automaticNetworks && it.match(ssid) == WifiTargetMatch.SAME && it.endpoints != null
    }
    private fun unclaimed() = candidates.values.filter {
        it.network in automaticNetworks && it.match(ssid) == WifiTargetMatch.SAME && it.endpoints != null
    }
    /**
     * The request's own network reaches its callback in the same dispatch as the observer, so a target
     * network the request has not claimed after [CLAIM_MILLIS] was joined by the platform itself (a saved
     * network). Such a request never attaches; borrow the platform's link instead of waiting it out.
     */
    private fun awaitClaim() {
        if (claim != null) return
        val token = generation
        claim = driver.later(CLAIM_MILLIS) {
            claim = null
            if (generation != token || phase != Phase.AUTOMATIC) return@later
            val unclaimed = unclaimed()
            if (unclaimed.size != 1) return@later
            ++generation
            timer?.close(); timer = null
            request?.close(); request = null
            automaticNetworks.remove(unclaimed.single().network)
            select(unclaimed.single())
        }
    }
    private fun automatic() {
        if (phase !in setOf(Phase.IDENTIFYING, Phase.MANUAL)) return
        timer?.close(); timer = null
        transition(Phase.AUTOMATIC)
        val token = ++generation
        val subscription = try { driver.request(object : Observer<N> {
            override fun changed(candidate: CarlinkWifiCandidate<N>) {
                if (generation != token || phase != Phase.AUTOMATIC) return
                if (candidate.match(ssid) == WifiTargetMatch.DIFFERENT) { manual(token); return }
                if (candidate.endpoints != null) select(candidate)
            }
            override fun lost(network: N) {
                if (generation == token && selected?.network == network) lost()
            }
        }, { manual(token) }, AUTOMATIC_MILLIS) } catch (_: Exception) {
            manual(token)
            return
        }
        if (generation != token || phase == Phase.CLOSED) subscription.close() else request = subscription
        if (phase == Phase.AUTOMATIC) timer = driver.later(AUTOMATIC_MILLIS.toLong()) { manual(token) }
    }
    private fun manual(token: Int) {
        if (generation != token || phase != Phase.AUTOMATIC) return
        ++generation
        timer?.close(); timer = null
        claim?.close(); claim = null
        request?.close(); request = null
        transition(Phase.MANUAL)
        // Once the request is gone its own network goes with it. What remains of the target was joined
        // by the platform during the request; adopt it instead of waiting for a return to the app.
        val token = generation
        timer = driver.later(CLAIM_MILLIS) {
            timer = null
            if (generation != token || phase != Phase.MANUAL) return@later
            val unclaimed = unclaimed()
            if (unclaimed.size == 1 && matching().isEmpty()) {
                automaticNetworks.remove(unclaimed.single().network)
                select(unclaimed.single())
            }
        }
    }
    private fun select(candidate: CarlinkWifiCandidate<N>) {
        if (phase == Phase.CLOSED || phase == Phase.READY) return
        check(candidate.endpoints != null)
        timer?.close(); timer = null
        claim?.close(); claim = null
        platformJoin = false
        selected = candidate
        transition(Phase.READY)
        result.complete(candidate)
    }
    private fun lost() {
        if (phase == Phase.CLOSED || lossReported) return
        lossReported = true
        connectionLost()
    }
    override fun close() {
        if (phase == Phase.CLOSED) return
        ++generation
        ++observationGeneration
        transition(Phase.CLOSED)
        timer?.close(); timer = null
        claim?.close(); claim = null
        request?.close(); request = null
        observer?.close(); observer = null
        result.cancel(false)
    }
    companion object {
        const val IDENTIFY_MILLIS = 250L
        const val CLAIM_MILLIS = 2_000L
        const val AUTOMATIC_MILLIS = 45_000
    }
}
