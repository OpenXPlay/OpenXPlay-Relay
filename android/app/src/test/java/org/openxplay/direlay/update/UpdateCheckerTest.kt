// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-25
package org.openxplay.direlay.update

import android.app.Application
import android.content.Context
import kotlinx.coroutines.*
import kotlinx.coroutines.flow.first
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config
import java.util.UUID
import java.util.concurrent.atomic.AtomicInteger

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [33], application = Application::class)
class UpdateCheckerTest {
    private fun preferences() = UpdatePreferences(RuntimeEnvironment.getApplication().getSharedPreferences(UUID.randomUUID().toString(), Context.MODE_PRIVATE))
    private suspend fun finished(checker: UpdateChecker) = withTimeout(5_000) { checker.state.first { !it.checking && (it.result != null || it.failure != null) } }

    @Test fun noAutomaticFetchAndOneCheckAtATimeAcrossCallers() = runBlocking {
        val started = CompletableDeferred<Unit>()
        val gate = CompletableDeferred<Unit>()
        val calls = AtomicInteger()
        val releases = ReleaseFeed.parse(ReleaseFixtures.feed())
        val checker = UpdateChecker(ReleaseSource { calls.incrementAndGet(); started.complete(Unit); gate.await(); releases }, preferences(), "0.1.2")
        assertEquals(UpdateState(UpdateChannel.STABLE), checker.state.value); assertEquals(0, calls.get())
        checker.setChannel(UpdateChannel.BETA)
        assertEquals(0, calls.get()); assertNull(checker.state.value.result)
        checker.check()
        try {
            withTimeout(5_000) { started.await() }
            coroutineScope { repeat(12) { launch(Dispatchers.Default) { checker.check() } } }
            assertTrue(checker.state.value.checking); assertEquals(1, calls.get())
        } finally { gate.complete(Unit) }
        val state = finished(checker)
        assertEquals("0.1.6-beta.1", (state.result as UpdateResult.Available).release.version.value)
        assertEquals(1, calls.get())
    }
    @Test fun switchingChannelReusesTheFeedAndARecreatedCheckerDoesNotPersistResults() = runBlocking {
        val calls = AtomicInteger()
        val releases = ReleaseFeed.parse(ReleaseFixtures.feed())
        val source = ReleaseSource { calls.incrementAndGet(); releases }
        val preferences = preferences()
        val checker = UpdateChecker(source, preferences, "0.1.5")
        checker.check()
        assertEquals("0.1.5", (finished(checker).result as UpdateResult.UpToDate).release.version.value)
        checker.setChannel(UpdateChannel.BETA)
        assertEquals("0.1.6-beta.1", (checker.state.value.result as UpdateResult.Available).release.version.value)
        checker.setChannel(UpdateChannel.STABLE)
        assertTrue(checker.state.value.result is UpdateResult.UpToDate)
        checker.setChannel(UpdateChannel.BETA)
        assertEquals(1, calls.get())
        val recreated = UpdateChecker(source, preferences, "0.1.5")
        assertEquals(UpdateState(UpdateChannel.BETA), recreated.state.value); assertEquals(1, calls.get())
    }
    @Test fun channelChangedDuringFetchDeterminesTheResultAndFailuresCanBeRetried() = runBlocking {
        val gate = CompletableDeferred<Unit>()
        val releases = ReleaseFeed.parse(ReleaseFixtures.feed())
        val calls = AtomicInteger()
        val checker = UpdateChecker(ReleaseSource {
            if (calls.incrementAndGet() == 1) throw UpdateFailure.NoInternet
            gate.await(); releases
        }, preferences(), "0.1.5")
        checker.check()
        assertEquals(UpdateFailure.NoInternet, finished(checker).failure)
        checker.check()
        assertNull(checker.state.value.failure)
        checker.setChannel(UpdateChannel.BETA)
        assertTrue(checker.state.value.checking)
        gate.complete(Unit)
        val state = finished(checker)
        assertEquals(UpdateChannel.BETA, state.channel)
        assertEquals("0.1.6-beta.1", (state.result as UpdateResult.Available).release.version.value)
        assertEquals(2, calls.get())
    }
}
