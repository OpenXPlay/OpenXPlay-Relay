// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-15
package org.openxplay.direlay.ui

import android.content.Context
import android.content.Intent
import android.os.Build
import android.provider.Settings

interface NetworkSettingsOpener { fun open(hotspot: Boolean) }
class NetworkSettingsLauncher(private val context: Context) : NetworkSettingsOpener {
    override fun open(hotspot: Boolean) {
        val actions = if (hotspot) listOf("android.settings.TETHER_SETTINGS", Settings.ACTION_WIRELESS_SETTINGS)
            else listOfNotNull(if (Build.VERSION.SDK_INT >= 29) Settings.Panel.ACTION_WIFI else null,
                Settings.ACTION_WIFI_SETTINGS, Settings.ACTION_WIRELESS_SETTINGS)
        for (action in actions.distinct()) {
            try {
                val intent = Intent(action)
                if (context !is android.app.Activity) intent.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
                context.startActivity(intent)
                return
            } catch (_: android.content.ActivityNotFoundException) { /* Try the next public settings page. */ }
            catch (_: SecurityException) { /* OEM settings may disallow a specific page. */ }
        }
    }
}
