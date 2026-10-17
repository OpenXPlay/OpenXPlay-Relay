// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-15
package org.openxplay.direlay.ui

import android.content.Context
import android.content.Intent
import android.provider.Settings

interface NetworkSettingsOpener { fun open(hotspot: Boolean) }
class NetworkSettingsLauncher(private val context: Context) : NetworkSettingsOpener {
    override fun open(hotspot: Boolean) {
        val action = if (hotspot) "android.settings.TETHER_SETTINGS" else Settings.ACTION_WIFI_SETTINGS
        try { context.startActivity(Intent(action)) }
        catch (_: android.content.ActivityNotFoundException) { context.startActivity(Intent(Settings.ACTION_WIRELESS_SETTINGS)) }
        catch (_: SecurityException) { context.startActivity(Intent(Settings.ACTION_WIRELESS_SETTINGS)) }
    }
}
