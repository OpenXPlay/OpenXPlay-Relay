// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-16
package org.openxplay.direlay.plugins

import com.shilapi.xcertplay.iap2.body.Iap2BodyReader
import com.shilapi.xcertplay.iap2.wire.Iap2Frame
import com.shilapi.xcertplay.orchestration.CarPlayEventObserver
import org.openxplay.stream.core.*
import java.util.concurrent.ArrayBlockingQueue
import java.util.concurrent.ThreadPoolExecutor
import java.util.concurrent.TimeUnit

/** Relay-owned translation of public DiPlay observations into Stream snapshots. Worker-confined. */
internal class CarPlayContext(emit: (ContextUpdate) -> Unit) {
    private val publish: (ContextUpdate) -> Unit = { update -> runCatching { emit(update) }; Unit }
    private var activity = ActivityContext(null, null, null)
    private var routeState: Int? = null
    private var route = RouteContext(false, null, null, null, null, null)
    private var road: String? = null
    private var current: Int? = null
    private var distance: Double? = null
    private val turns = linkedMapOf<Int, RouteManeuver>()

    fun session(active: Boolean) {
        if (!active) {
            activity = ActivityContext(false, false, false)
            routeState = null; road = null; current = null; distance = null; turns.clear()
            route = RouteContext(false, null, null, null, null, null)
            publish(ContextUpdate.Activity(activity))
            publish(ContextUpdate.Route(null))
            publish(ContextUpdate.AudioSource(null))
        }
        publish(ContextUpdate.Session(SessionContext(if (active) CarPlayState.CONNECTED else CarPlayState.DISCONNECTED)))
    }

    fun command(type: String, params: Map<String, Any?>) {
        if (type != "modesChanged") return
        val modes = if (params.containsKey("modes")) params["modes"] as? Map<*, *> ?: return else params
        val states = modes["appStates"] as? List<*> ?: return
        var next = activity
        fun integer(value: Any?): Int? = (value as? Number)?.toDouble()?.let { n ->
            n.takeIf { it.isFinite() && it == it.toInt().toDouble() }?.toInt()
        }
        states.forEach { entry ->
            val state = entry as? Map<*, *> ?: return@forEach
            val owner = integer(state["entity"])?.takeIf { it in 0..2 }?.let { it == 1 }
            if (owner == null) return@forEach
            when (integer(state["appStateID"])) {
                1 -> integer(state["speechMode"])?.takeIf { it in -1..3 }?.let { mode ->
                    next = next.copy(siri = owner && mode > 0)
                }
                2 -> next = next.copy(phoneCall = owner)
                3 -> next = next.copy(navigation = owner)
            }
        }
        if (next != activity) { activity = next; publish(ContextUpdate.Activity(next)) }
    }

    fun frame(frame: Iap2Frame) {
        when (frame.messageId) {
            0x5001 -> {
                // DiPlay CarPlayPlaybackStatus: playback group 1, SOURCE_APP field 7.
                val body = Iap2BodyReader.of(frame)
                require(body.all(1).size <= 1)
                val playback = body.optionalGroup(1) ?: return
                require(playback.all(7).size <= 1)
                if (playback.has(7)) publish(ContextUpdate.AudioSource(
                    AudioSourceContext(text(playback, 7)?.ifEmpty { null })))
            }
            0x5201, 0x5202 -> routeFrame(frame)
        }
    }

    private fun text(body: Iap2BodyReader, id: Int): String? = body.optionalBytes(id)?.let { bytes ->
        require(bytes.isNotEmpty() && bytes.last() == 0.toByte() && bytes.dropLast(1).none { it == 0.toByte() })
        Charsets.UTF_8.newDecoder().decode(java.nio.ByteBuffer.wrap(bytes, 0, bytes.size - 1)).toString().trim()
    }

    private fun routeFrame(frame: Iap2Frame) {
        // Field meanings: DiPlay BydHudRouteState (route/turn details), FceNavigationState (source/state).
        // The route readers are internal; call the public wire reader rather than duplicating its codec.
        val b = Iap2BodyReader.of(frame)
        val ids = if (frame.messageId == 0x5201) listOf(1, 3, 6, 7, 10, 13, 19) else listOf(1, 3, 4)
        require(ids.all { b.all(it).size <= 1 })
        if (ids.none(b::has)) return
        if (frame.messageId == 0x5202) {
            val index = b.optionalU16(1)
            val kind = b.optionalU8(3)
            val afterRoad = text(b, 4)
            if (index == null) return
            val previous = turns[index]
            if (previous == null && turns.size == 8) turns.remove(turns.keys.first())
            turns[index] = RouteManeuver(kind ?: previous?.type, afterRoad ?: previous?.road, null)
        } else {
            // Decode every used field before modifying retained state: malformed updates are atomic.
            val state = b.optionalU8(1)
            val street = text(b, 3)
            val seconds = b.optionalU64(6)?.let { java.lang.Long.toUnsignedString(it).toDouble() }
            val meters = b.optionalU32(7)?.toDouble()
            val turnMeters = b.optionalU32(10)?.toDouble()
            val indices = b.optionalU16List(13)
            val app = text(b, 19)
            routeState = state ?: routeState
            route = route.copy(appName = app ?: route.appName)
            if (routeState == 0 || routeState == 2 || route.appName == "") {
                road = null; current = null; distance = null; turns.clear()
                route = route.copy(etaMs = null, distanceRemainingM = null, maneuver = null)
            } else {
                if (indices != null && indices.firstOrNull() != current && turnMeters == null) distance = null
                if (indices != null) current = indices.firstOrNull()
                if (turnMeters != null) distance = turnMeters
                if (street != null) road = street
                route = route.copy(etaMs = seconds?.times(1000) ?: route.etaMs,
                    distanceRemainingM = meters ?: route.distanceRemainingM)
            }
        }
        val active = route.appName != "" && (routeState?.let { it in setOf(1, 3, 4, 5, 6) }
            ?: (route.appName != null || current != null))
        val snapshot = if (!active) RouteContext(false, null, null, null, null, null) else route.copy(active = true,
            appName = route.appName?.ifEmpty { null }, maneuver = current?.let { index ->
                val turn = turns[index]
                RouteManeuver(turn?.type, turn?.road?.takeIf { it.isNotEmpty() } ?: road?.takeIf { it.isNotEmpty() }, distance)
            })
        publish(ContextUpdate.Route(snapshot))
    }
}

/** A bounded, isolated worker: callbacks only enqueue; parsing and Stream calls never run on CarPlay threads. */
class PluginContextObserver(private val publish: (ContextUpdate) -> Unit) : CarPlayEventObserver, AutoCloseable {
    private val generation = java.util.concurrent.atomic.AtomicLong()
    @Volatile private var enabled = false
    private var decoderGeneration = -1L
    private var decoder = CarPlayContext(publish)
    private val worker = ThreadPoolExecutor(1, 1, 0, TimeUnit.MILLISECONDS, ArrayBlockingQueue<Runnable>(128),
        { task -> Thread(task, "direlay-plugin-context").apply { isDaemon = true } }, ThreadPoolExecutor.AbortPolicy())

    fun configure(value: Boolean, active: Boolean = false) {
        enabled = false
        generation.incrementAndGet()
        worker.queue.clear()
        enabled = value
        if (value) offer { decoder -> decoder.session(false); if (active) decoder.session(true) }
    }
    private fun offer(action: (CarPlayContext) -> Unit) {
        if (!enabled) return
        val epoch = generation.get()
        runCatching { worker.execute {
            if (enabled && epoch == generation.get()) runCatching {
                if (decoderGeneration != epoch) {
                    decoder = CarPlayContext { update -> if (enabled && epoch == generation.get()) publish(update) }
                    decoderGeneration = epoch
                }
                action(decoder)
            }
        } }
    }
    override fun onSessionActive() = offer { it.session(true) }
    override fun onSessionEnded() {
        // Teardown supersedes queued deltas, including when the bounded queue is full.
        generation.incrementAndGet()
        worker.queue.clear()
        offer { it.session(false) }
    }
    override fun onIap2Frame(frame: Iap2Frame) {
        if (frame.messageId in listOf(0x5001, 0x5201, 0x5202)) offer { it.frame(frame) }
    }
    override fun onCommand(type: String, params: Map<String, Any?>) {
        if (type == "modesChanged") offer { it.command(type, params) }
    }
    override fun close() { configure(false); worker.shutdownNow() }
}

/** Android deliberately provides no media metadata or call-state topic, including hub idle replay. */
class AndroidPluginOutput(private val output: ProjectionOutput) : ProjectionOutput by output {
    override fun onContext(update: ContextUpdate) {
        when (update.topic) {
            ContextTopic.SESSION, ContextTopic.ACTIVITY, ContextTopic.ROUTE, ContextTopic.AUDIO_SOURCE -> output.onContext(update)
            else -> Unit
        }
    }
}
