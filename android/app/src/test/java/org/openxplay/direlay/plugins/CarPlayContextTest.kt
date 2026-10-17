// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-16
package org.openxplay.direlay.plugins

import com.shilapi.xcertplay.iap2.wire.Iap2Frame
import com.shilapi.xcertplay.iap2.wire.Iap2Parameter
import com.shilapi.xcertplay.iap2.wire.Iap2ParameterList
import org.junit.Assert.*
import org.junit.Test
import org.openxplay.stream.core.*
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit

class CarPlayContextTest {
    private fun body(vararg values: Pair<Int, ByteArray>) = Iap2ParameterList.of(values.map { Iap2Parameter(it.first, it.second) }).encode()
    private fun frame(id: Int, vararg values: Pair<Int, ByteArray>) = Iap2Frame(id, body(*values))
    private fun bytes(vararg value: Int) = value.map(Int::toByte).toByteArray()
    private fun text(value: String) = (value + '\u0000').toByteArray()
    private fun number(value: Long, size: Int) = ByteArray(size) { index -> (value ushr ((size - index - 1) * 8)).toByte() }

    @Test fun modesArePartialAndRespectOwnersAndSpeechModes() {
        val updates = mutableListOf<ContextUpdate>()
        val context = CarPlayContext(updates::add)
        fun command(vararg states: Map<String, Any?>) = context.command("modesChanged", mapOf("modes" to mapOf("appStates" to states.toList())))
        command(mapOf("appStateID" to 3, "entity" to 1), mapOf("appStateID" to 2, "entity" to 2),
            mapOf("appStateID" to 1, "entity" to 1, "speechMode" to 2.0))
        assertEquals(ContextUpdate.Activity(ActivityContext(true, false, true)), updates.last())
        command(mapOf("appStateID" to 3, "entity" to 99), mapOf("appStateID" to 1, "entity" to 1, "speechMode" to 4))
        assertEquals(1, updates.size)
        command(mapOf("appStateID" to 1, "entity" to 1, "speechMode" to -1))
        assertEquals(ContextUpdate.Activity(ActivityContext(false, false, true)), updates.last())
        for (owner in 0..2) {
            command(mapOf("appStateID" to 3, "entity" to owner))
            assertEquals(owner == 1, (updates.last() as ContextUpdate.Activity).value!!.navigation)
        }
        context.command("requestUI", mapOf("appStates" to emptyList<Any>()))
    }

    @Test fun syntheticRouteFramesMergeAndMalformedFramesDoNotMutateState() {
        val updates = mutableListOf<ContextUpdate>()
        val context = CarPlayContext(updates::add)
        context.frame(frame(0x5202, 1 to bytes(0, 7), 3 to bytes(2), 4 to text("Next St")))
        context.frame(frame(0x5201, 1 to bytes(1), 3 to text("Main St"), 6 to number(900, 8),
            7 to number(12000, 4), 10 to number(250, 4), 13 to bytes(0, 7), 19 to text("Maps")))
        assertEquals(RouteContext(true, null, 900000.0, 12000.0, RouteManeuver(2, "Next St", 250.0), "Maps"),
            (updates.last() as ContextUpdate.Route).value)
        val before = updates.last()
        for (bad in listOf(frame(0x5201, 1 to bytes(0), 6 to bytes(1)),
            frame(0x5201, 1 to bytes(0), 1 to bytes(1)), frame(0x5201, 3 to bytes(255, 0)),
            frame(0x5201, 13 to bytes(1)))) assertTrue(runCatching { context.frame(bad) }.isFailure)
        context.frame(frame(0x5201, 99 to bytes(1)))
        assertEquals(before, updates.last())
        context.frame(frame(0x5201, 13 to bytes(0, 8)))
        assertEquals(RouteManeuver(null, "Main St", null), (updates.last() as ContextUpdate.Route).value!!.maneuver)
        context.frame(frame(0x5201, 13 to bytes()))
        assertNull((updates.last() as ContextUpdate.Route).value!!.maneuver)
        context.frame(frame(0x5201, 1 to bytes(2)))
        assertFalse((updates.last() as ContextUpdate.Route).value!!.active)
        context.session(false)
        assertEquals(ContextUpdate.Session(SessionContext(CarPlayState.DISCONNECTED)), updates.last())
        assertTrue(updates.none { it is ContextUpdate.NowPlaying || it is ContextUpdate.Call })
    }

    @Test fun sourceAppOnlyUsesPlaybackFieldSevenAndClearsOnEnd() {
        val updates = mutableListOf<ContextUpdate>()
        val context = CarPlayContext(updates::add)
        context.session(true)
        assertEquals(ContextUpdate.Session(SessionContext(CarPlayState.CONNECTED)), updates.last())
        context.frame(frame(0x5001, 1 to body(7 to text("Music"))))
        assertEquals(ContextUpdate.AudioSource(AudioSourceContext("Music")), updates.last())
        val count = updates.size
        context.frame(frame(0x5001, 0 to body(1 to text("Private song"))))
        assertEquals(count, updates.size)
        context.frame(frame(0x5001, 1 to body(7 to text(""))))
        assertEquals(ContextUpdate.AudioSource(AudioSourceContext(null)), updates.last())
        context.session(false)
        assertTrue(updates.contains(ContextUpdate.AudioSource(null)))
    }

    @Test fun observerIsOffByDefaultAndCallbacksNeverWaitForConsumerOrThrow() {
        val entered = CountDownLatch(1)
        val release = CountDownLatch(1)
        val ended = CountDownLatch(1)
        val observer = PluginContextObserver { update ->
            entered.countDown()
            release.await(5, TimeUnit.SECONDS)
            if (update == ContextUpdate.Session(SessionContext(CarPlayState.DISCONNECTED))) ended.countDown()
            throw IllegalStateException("failed consumer")
        }
        try {
            observer.onSessionActive()
            assertFalse(entered.await(50, TimeUnit.MILLISECONDS))
            observer.configure(true)
            observer.onSessionActive()
            assertTrue(entered.await(2, TimeUnit.SECONDS))
            val begin = System.nanoTime()
            repeat(1000) { observer.onIap2Frame(frame(0x5201, 1 to bytes(1))) }
            observer.onSessionEnded()
            assertTrue(TimeUnit.NANOSECONDS.toMillis(System.nanoTime() - begin) < 1000)
            release.countDown()
            assertTrue(ended.await(2, TimeUnit.SECONDS))
        } finally { release.countDown(); observer.close() }
        observer.onSessionActive()
    }

    @Test fun outputForwardsOnlyTheFourAndroidTopics() {
        val received = mutableListOf<ContextUpdate>()
        val inner = object : ProjectionOutput by RecordingOutput() {
            override fun wantsContext() = true
            override fun onContext(update: ContextUpdate) { received.add(update) }
        }
        val output = AndroidPluginOutput(inner)
        ContextUpdate.idle().forEach(output::onContext)
        assertTrue(output.wantsContext())
        assertEquals(listOf(ContextTopic.SESSION, ContextTopic.ACTIVITY, ContextTopic.ROUTE, ContextTopic.AUDIO_SOURCE), received.map { it.topic })
    }
}
