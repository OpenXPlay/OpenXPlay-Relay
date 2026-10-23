// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-22
package org.openxplay.direlay.miracast

import android.content.Context
import android.content.Intent
import android.net.Uri
import android.provider.Settings

object MiracastSystem {
    fun canDrawOverlays(context: Context): Boolean = Settings.canDrawOverlays(context)

    fun overlayPermissionIntent(context: Context): Intent =
        Intent(Settings.ACTION_MANAGE_OVERLAY_PERMISSION, Uri.parse("package:${context.packageName}"))
            .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)

    fun openCastSettings(context: Context): Boolean {
        for (action in listOf(Settings.ACTION_CAST_SETTINGS, "android.settings.WIFI_DISPLAY_SETTINGS", Settings.ACTION_WIRELESS_SETTINGS)) {
            val intent = Intent(action).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
            try {
                context.startActivity(intent)
                return true
            } catch (_: android.content.ActivityNotFoundException) { }
            catch (_: SecurityException) { }
        }
        return false
    }
}
