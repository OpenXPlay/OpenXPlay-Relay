// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-23
package org.openxplay.direlay.logs

import java.util.concurrent.ConcurrentLinkedQueue
import java.util.concurrent.CountDownLatch
import java.util.concurrent.Semaphore
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicBoolean
import java.util.concurrent.atomic.AtomicLong

/** Producers only reserve a slot and offer; encoding, sinks and failures belong to the daemon. */
class BoundedLogWorker(name: String, private val capacity: Long = 1024,
    private val warning: (String) -> Unit = {}) : AutoCloseable {
    private val queue = ConcurrentLinkedQueue<() -> Unit>()
    private val pending = AtomicLong()
    private val accepting = AtomicBoolean(true)
    private val wake = Semaphore(0)
    val dropped = AtomicLong()
    private var warned = false
    private var reportedDrops = 0L
    private val thread = Thread({
        while (accepting.get() || pending.get() > 0) {
            wake.tryAcquire(100, TimeUnit.MILLISECONDS); wake.drainPermits()
            while (true) {
                val action = queue.poll() ?: break
                try {
                    action()
                    val count = dropped.get()
                    if (count != reportedDrops) { reportedDrops = count; warning("dropped=$count") }
                } catch (_: Throwable) {
                    if (!warned) { warned = true; runCatching { warning("logging_failed") } }
                } finally { pending.decrementAndGet() }
            }
        }
    }, name).apply { isDaemon = true; start() }

    fun offer(action: () -> Unit): Boolean {
        if (!accepting.get()) return false
        if (pending.incrementAndGet() > capacity) { pending.decrementAndGet(); dropped.incrementAndGet(); return false }
        queue.add(action); wake.release()
        return true
    }
    internal fun awaitIdle() {
        val done = CountDownLatch(1)
        check(offer { done.countDown() }); check(done.await(5, TimeUnit.SECONDS))
    }
    override fun close() { accepting.set(false); wake.release() }
}
