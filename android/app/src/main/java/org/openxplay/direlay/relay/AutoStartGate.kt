// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-15
package org.openxplay.direlay.relay

interface AutoStartGate {
    val paused: Boolean
    fun pause()
    fun resume()
}
fun interface BootIdentitySource { fun identity(): String }
interface AutoStartStorage {
    fun read(key: String): String?
    fun write(key: String, value: String)
}

fun bootIdentity(count: Int?, wallTime: Long, elapsedTime: Long): String =
    count?.takeIf { it >= 0 }?.let { "count:$it" }
        ?: "time:${Math.round((wallTime - elapsedTime) / 10_000.0) * 10_000L}"

/** A pause belongs to one boot, including across process recreation. */
class PersistentAutoStartGate(boot: BootIdentitySource, private val storage: AutoStartStorage,
    onBootReset: () -> Unit = {}) : AutoStartGate {
    private val identity = boot.identity()
    private val previousPause = storage.read("pause")
    @Volatile override var paused = previousPause == identity
        private set
    init {
        if (!previousPause.isNullOrEmpty() && previousPause != identity) {
            storage.write("pause", ""); onBootReset()
        }
    }
    @Synchronized override fun pause() { storage.write("pause", identity); paused = true }
    @Synchronized override fun resume() { storage.write("pause", ""); paused = false }
}

/** Remembers a live service across process death and consumes only our deliberate restart stop. */
class SessionStopTracker(boot: BootIdentitySource, private val storage: AutoStartStorage,
    private val stopped: () -> Unit) {
    private val identity = boot.identity()
    private var wasRunning = storage.read("running") == identity
    private var restart = storage.read("restart") == identity
    @Volatile var running = false
        private set
    @Volatile var observed = false
        private set
    @Synchronized fun restarting() { storage.write("restart", identity); restart = true }
    @Synchronized fun observe(value: Boolean) {
        if (wasRunning && !value) {
            if (!restart) stopped()
        }
        if (!value && restart) { storage.write("restart", ""); restart = false }
        if (wasRunning != value) storage.write("running", if (value) identity else "")
        wasRunning = value; running = value; observed = true
    }
}
