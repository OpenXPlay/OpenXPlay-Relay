// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-11-02
package org.openxplay.direlay.carlife

import java.io.Closeable

/** Fences a blocking platform open that completes after detach or output shutdown. */
internal class AccessoryAttachment<T> : Closeable {
    private var epoch = 0L
    private var current: Pair<T, Closeable>? = null
    @Volatile var closed = false
        private set

    @Synchronized fun generation(): Long = epoch
    @Synchronized fun isCurrent(generation: Long): Boolean = !closed && epoch == generation
    @Synchronized fun contains(device: T): Boolean = current?.first == device

    fun install(device: T, generation: Long, lease: Closeable): Boolean {
        val accepted = synchronized(this) {
            if (closed || epoch != generation || current != null) false
            else { current = device to lease; true }
        }
        if (!accepted) lease.close()
        return accepted
    }

    fun clear(): Boolean {
        val previous = synchronized(this) {
            epoch++
            current.also { current = null }
        }
        previous?.second?.close()
        return previous != null
    }

    override fun close() {
        synchronized(this) { closed = true }
        clear()
    }
}
