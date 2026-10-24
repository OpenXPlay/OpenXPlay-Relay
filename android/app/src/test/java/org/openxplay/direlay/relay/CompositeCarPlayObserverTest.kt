// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-22
package org.openxplay.direlay.relay

import com.shilapi.xcertplay.iap2.wire.Iap2Frame
import com.shilapi.xcertplay.orchestration.CarPlayEventObserver
import com.shilapi.xcertplay.orchestration.CarPlayStatus
import org.junit.Assert.*
import org.junit.Test

class CompositeCarPlayObserverTest {
    private class Recording(private val name: String, private val calls: MutableList<String>,
        private val throwing: Boolean = false) : CarPlayEventObserver {
        private fun record(event: String) { calls.add("$name:$event"); if (throwing) error("observer failed") }
        override fun onStatus(status: CarPlayStatus) = record("status")
        override fun onProgress(message: String) = record("progress")
        override fun onSessionActive() = record("active")
        override fun onSessionEnded() = record("ended")
        override fun onIap2Frame(frame: Iap2Frame) = record("frame")
        override fun onCommand(type: String, params: Map<String, Any?>) = record("command")
    }
    private fun emit(observer: CarPlayEventObserver) {
        observer.onStatus(CarPlayStatus.DiscoveringMfi)
        observer.onProgress("connecting")
        observer.onSessionActive()
        observer.onSessionEnded()
        observer.onIap2Frame(Iap2Frame(1, byteArrayOf()))
        observer.onCommand("requestUI", emptyMap())
    }
    @Test fun pluginsOffStillRecordsDiagnostics() {
        val calls = mutableListOf<String>()
        emit(CompositeCarPlayObserver(Recording("diagnostics", calls)))
        assertEquals(listOf("status", "progress", "active", "ended", "frame", "command").map { "diagnostics:$it" }, calls)
    }
    @Test fun pluginsOnCallsBothInOrderEvenWhenEitherThrows() {
        for (throwDiagnostics in listOf(false, true)) {
            val calls = mutableListOf<String>()
            emit(CompositeCarPlayObserver(Recording("diagnostics", calls, throwDiagnostics),
                Recording("plugins", calls, !throwDiagnostics)))
            assertEquals(listOf("status", "progress", "active", "ended", "frame", "command").flatMap {
                listOf("diagnostics:$it", "plugins:$it")
            }, calls)
        }
    }

    @Test fun everyObserverIsIsolatedAndStageIsForwardedWithPluginsOffOrOn() {
        for (plugins in listOf(false, true)) for (throwing in listOf("diagnostics", "stage", "link", "plugins")) {
            val calls = mutableListOf<String>()
            emit(CompositeCarPlayObserver(Recording("diagnostics", calls, throwing == "diagnostics"),
                Recording("plugins", calls, throwing == "plugins").takeIf { plugins },
                stage = Recording("stage", calls, throwing == "stage"),
                link = Recording("link", calls, throwing == "link")))
            assertEquals(listOf("status", "progress", "active", "ended", "frame", "command").flatMap { event ->
                listOfNotNull("diagnostics:$event", "stage:$event", "link:$event", "plugins:$event".takeIf { plugins })
            }, calls)
        }
    }
}
