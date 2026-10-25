// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-25
package org.openxplay.direlay.presentation

import kotlinx.coroutines.*
import kotlinx.coroutines.flow.*
import org.junit.Assert.*
import org.junit.Test
import org.openxplay.direlay.path.*
import org.openxplay.direlay.ui.*

class HomePublicationTest {
    private fun sample(input: PathInput): HomeContent {
        val path = presentPath(input)
        return publishHome(input, path, HomeContent(setupHomePresentation(input, path, emptyList()),
            emptyList(), "Relay", false, false, null))
    }

    @Test fun clockOnlySamplesPublishOnceAndRuntimeChangesPublishAgain() = runBlocking {
        val input = PathInput(nowMillis = 1000, version = "0.1.6")
        val state = MutableStateFlow<HomeContent?>(null)
        val published = mutableListOf<HomeContent>()
        val job = launch(start = CoroutineStart.UNDISPATCHED) { state.filterNotNull().collect { published.add(it) } }
        try {
            state.value = sample(input)
            yield()
            state.value = sample(input.copy(nowMillis = 9999))
            yield()
            assertEquals(1, published.size)
            assertEquals(0L, published.single().pathInput?.nowMillis)
            assertFalse(published.single().path.diagnostic.contains("0.1.6"))
            state.value = sample(input.copy(nowMillis = 10000, relay = input.relay.copy(bluetoothName = "New relay")))
            yield()
            assertEquals(2, published.size)
            assertEquals("New relay", published.last().path.nodes.relay)
        } finally { job.cancelAndJoin() }
    }

    @Test fun countdownPublishesItsDeadlineAndKeepsTheTitleResource() {
        val input = PathInput(nowMillis = 1000, iphone = IphoneInput(started = true, carWaitUntilMillis = 21000))
        val first = sample(input)
        assertEquals(first, sample(input.copy(nowMillis = 5500)))
        val step = first.path.steps.first { it.id == StepId.CARPLAY_START }
        assertEquals(21000L, step.carWaitUntilMillis)
        assertEquals(StepText.of(org.openxplay.direlay.R.string.path_car_screen_wait), step.detail)
        assertEquals(StepId.CARPLAY_START.titleResource(), step.title)
    }
}
