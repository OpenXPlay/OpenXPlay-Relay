// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-15
package org.openxplay.direlay.relay

import android.app.ActivityManager
import android.content.Context
import android.os.SystemClock
import android.provider.Settings
import android.util.Log
import com.shilapi.xcertplay.DiPlaySessionService

class AndroidAutoStartStorage(context: Context) : AutoStartStorage {
    private val preferences = context.getSharedPreferences("direlay-auto-start", Context.MODE_PRIVATE)
    override fun read(key: String): String? = preferences.getString(key, null)
    override fun write(key: String, value: String) {
        check(preferences.edit().putString(key, value).commit()) { "Could not persist automatic start state" }
    }
}
class AndroidBootIdentity(private val context: Context) : BootIdentitySource {
    private val value by lazy { bootIdentity(runCatching {
        Settings.Global.getInt(context.contentResolver, Settings.Global.BOOT_COUNT)
    }.getOrNull(), System.currentTimeMillis(), SystemClock.elapsedRealtime()) }
    override fun identity(): String = value
}

/** The service remains observable while the host screen covers the relay activity. */
class AutoStartSessions(context: Context, val gate: AutoStartGate, boot: BootIdentitySource, storage: AutoStartStorage) {
    private val manager = context.getSystemService(ActivityManager::class.java)
    val tracker = SessionStopTracker(boot, storage) { pause("session stopped") }
    fun pause(reason: String) {
        if (!gate.paused) { gate.pause(); Log.i("OpenXPlay Relay", "Automatic start paused: $reason") }
    }
    fun resume(reason: String) {
        gate.resume(); Log.i("OpenXPlay Relay", "Automatic start resumed: $reason")
    }
    fun refresh() {
        // A failed platform read is not evidence that a session stopped.
        @Suppress("DEPRECATION")
        val running = runCatching { manager.getRunningServices(Int.MAX_VALUE).any {
            it.service.className == DiPlaySessionService::class.java.name && it.started
        } }.getOrNull() ?: return
        tracker.observe(running)
    }
}
