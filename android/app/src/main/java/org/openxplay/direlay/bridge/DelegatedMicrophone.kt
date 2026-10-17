// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-14
package org.openxplay.direlay.bridge

import android.util.Log
import com.shilapi.xcertplay.airplay.*
import java.util.concurrent.atomic.AtomicBoolean
import java.util.concurrent.locks.LockSupport

/** DiPlay's recorder setup and shutdown can block, so both run outside receive threads. */
internal class DelegatedMicrophone(private val sink: MediaSink, private val id: AudioStreamId,
    private val config: MicrophoneConfig, private var predecessor: DelegatedMicrophone? = null) : AutoCloseable {
    private val ended = java.util.concurrent.CountDownLatch(1)
    private val running = AtomicBoolean(true)
    private val worker = Thread({
        var started = false
        try {
            predecessor?.ended?.await(); predecessor = null
            if (running.get()) { started = true; sink.onMicrophoneStarted(id, config) }
            while (running.get()) LockSupport.park()
        } catch (failure: Exception) { Log.e("DiRelayMic", "Local microphone failed", failure) }
        finally { if (started) runCatching { sink.onMicrophoneStopped(id) }; ended.countDown() }
    }, "direlay-local-microphone").apply { isDaemon = true }
    fun start() = worker.start()
    override fun close() { running.set(false); LockSupport.unpark(worker) }
}
