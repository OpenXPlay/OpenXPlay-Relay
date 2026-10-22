// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-22
package org.openxplay.direlay.service

import android.content.Context
import android.content.Intent
import android.util.Log
import androidx.core.content.ContextCompat
import org.openxplay.direlay.relay.RelayKeepAlive
import org.openxplay.direlay.relay.RelayKeepAlivePolicy

class AndroidRelayKeepAlive(context: Context) : RelayKeepAlive {
    private val context = context.applicationContext
    private fun intent() = Intent(context, RelayKeepAliveService::class.java)
    private val policy = RelayKeepAlivePolicy(start = {
        try {
            ContextCompat.startForegroundService(this.context, intent())
            true
        } catch (failure: IllegalStateException) {
            // Includes ForegroundServiceStartNotAllowedException on Android 12+.
            Log.w(TAG, "Keep-alive start refused", failure)
            false
        } catch (failure: SecurityException) {
            Log.w(TAG, "Keep-alive start refused", failure)
            false
        }
    }, stop = { this.context.stopService(intent()); Unit })

    @Synchronized override fun update(needed: Boolean) { policy.update(needed) }
    @Synchronized override fun uiVisible() { policy.uiVisible() }
    @Synchronized fun foregroundStarted() { policy.foregroundStarted() }
    @Synchronized fun foregroundRefused() { policy.foregroundRefused() }

    private companion object { const val TAG = "DiRelayKeepAlive" }
}
