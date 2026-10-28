// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-14
package org.openxplay.direlay

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.util.Log
import com.shilapi.xcertplay.AirPlayPersistence

/** Boot starts the relay's readiness flow rather than DiPlay's removed home screen. */
class RelayBootReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        if (intent.action != Intent.ACTION_BOOT_COMPLETED || !AirPlayPersistence.loadAutoStartOnBoot(context)) return
        try {
            context.startActivity(RelayActivity.intent(context).apply {
                putExtra(RelayActivity.EXTRA_AUTO_START, true)
            })
        } catch (failure: RuntimeException) {
            Log.w("OpenXPlay Relay", "Android prevented the boot launch; open OpenXPlay Relay to start CarPlay", failure)
        }
    }
}
