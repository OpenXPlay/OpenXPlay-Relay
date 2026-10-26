// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-25
package org.openxplay.direlay.ui.design

import android.app.Application
import androidx.compose.runtime.*
import androidx.compose.runtime.snapshots.Snapshot
import kotlinx.coroutines.*
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import org.openxplay.direlay.ui.LocalRelayMotion
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [33], application = Application::class)
class BoundedMotionTest {
    private class EmptyApplier : AbstractApplier<Unit>(Unit) {
        override fun insertTopDown(index: Int, instance: Unit) = Unit
        override fun insertBottomUp(index: Int, instance: Unit) = Unit
        override fun remove(index: Int, count: Int) = Unit
        override fun move(from: Int, to: Int, count: Int) = Unit
        override fun onClear() = Unit
    }

    @Test fun phaseAndBreathSettleAndRestartWhenTheirStateChanges() = runBlocking {
        withTimeout(10000) {
            val clock = BroadcastFrameClock()
            val recomposer = Recomposer(coroutineContext + clock)
            val runner = launch(clock) { recomposer.runRecomposeAndApplyChanges() }
            val composition = Composition(EmptyApplier(), recomposer)
            val key = mutableStateOf(0)
            var phase: State<Float>? = null
            var breath: State<Float>? = null
            var now = 0L
            suspend fun frame() {
                Snapshot.sendApplyNotifications()
                repeat(3) { yield() }
                now += 100_000_000L
                clock.sendFrame(now)
                repeat(3) { yield() }
            }
            try {
                composition.setContent {
                    phase = rememberRelayPhase(true, key.value, durationMillis = 8000)
                    breath = rememberRelayBreath(true, key.value)
                }
                repeat(5) { frame() }
                assertTrue(clock.hasAwaiters)
                repeat(160) { frame() }
                assertFalse("Settled motion must not request frames", clock.hasAwaiters)
                assertEquals(1f, checkNotNull(breath).value, .0001f)
                val settled = checkNotNull(phase).value
                repeat(5) { frame() }
                assertEquals(settled, checkNotNull(phase).value, 0f)
                key.value++
                repeat(5) { frame() }
                assertTrue("A new state restarts motion", clock.hasAwaiters)
                repeat(160) { frame() }
                assertFalse(clock.hasAwaiters)
            } finally {
                composition.dispose()
                recomposer.cancel()
                runner.cancelAndJoin()
            }
        }
    }

    @Test fun liveAndNetworkPhasesFinishAfterExactlyTwoPasses() = runBlocking {
        for (period in listOf(1600L, 6000L)) {
            var frames = 0
            val clock = object : MonotonicFrameClock {
                override suspend fun <R> withFrameNanos(onFrame: (Long) -> R): R =
                    onFrame(frames++.toLong() * 100_000_000L)
            }
            var phase = -1f
            withContext(clock) { runRelayPhase(period, period * 2) { phase = it } }
            assertEquals((period * 2 / 100 + 1).toInt(), frames)
            assertEquals(0f, phase, 0f)
        }
    }

    @Test fun disabledMotionRequestsNoAnimationFrames() = runBlocking {
        val clock = BroadcastFrameClock()
        val recomposer = Recomposer(coroutineContext + clock)
        val runner = launch(clock) { recomposer.runRecomposeAndApplyChanges() }
        val composition = Composition(EmptyApplier(), recomposer)
        var phase: State<Float>? = null
        var breath: State<Float>? = null
        try {
            composition.setContent {
                CompositionLocalProvider(LocalRelayMotion provides false) {
                    phase = rememberRelayPhase(true)
                    breath = rememberRelayBreath(true)
                }
            }
            repeat(5) { yield(); clock.sendFrame(it * 100_000_000L) }
            assertFalse(clock.hasAwaiters)
            assertEquals(0f, checkNotNull(phase).value, 0f)
            assertEquals(1f, checkNotNull(breath).value, 0f)
        } finally { composition.dispose(); recomposer.cancel(); runner.cancelAndJoin() }
    }
}
