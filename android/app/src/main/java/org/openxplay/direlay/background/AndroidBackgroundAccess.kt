// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-29
package org.openxplay.direlay.background

import android.app.ActivityManager
import android.content.ComponentName
import android.content.Context
import android.content.Intent
import android.net.Uri
import android.os.Build
import android.os.PowerManager
import android.provider.Settings

/** Read and resolve on the presentation worker; keep the usable pages until the next resume. */
class AndroidBackgroundAccess(context: Context) {
    private val context = context.applicationContext
    private var vendorPages = emptyList<VendorBackgroundPage>()
    private val packageUri get() = Uri.parse("package:${context.packageName}")

    fun read(): BackgroundStatus {
        val manager = context.packageManager
        vendorPages = VendorBackgroundCatalogue.candidates(Build.MANUFACTURER, Build.BRAND, context.packageName,
            context.applicationInfo.loadLabel(manager).toString()).filter { page ->
            try { manager.resolveActivity(vendorIntent(page), 0)?.activityInfo?.exported == true }
            catch (_: SecurityException) { false }
        }
        val restricted = Build.VERSION.SDK_INT >= 28 &&
            context.getSystemService(ActivityManager::class.java)?.isBackgroundRestricted == true
        return BackgroundStatus(batteryExempt(), restricted, vendorPages.map { it.kind }.toSet())
    }

    fun intents(destination: BackgroundDestination): List<Intent> {
        val pages = when (destination) {
            BackgroundDestination.BATTERY_OPTIMIZATION -> buildList {
                if (!batteryExempt()) add(Intent(Settings.ACTION_REQUEST_IGNORE_BATTERY_OPTIMIZATIONS, packageUri))
                add(Intent(Settings.ACTION_IGNORE_BATTERY_OPTIMIZATION_SETTINGS))
            }
            BackgroundDestination.APP_DETAILS -> emptyList()
            BackgroundDestination.AUTOSTART -> vendorPages.filter { it.kind == VendorPageKind.AUTOSTART }.map(::vendorIntent)
            BackgroundDestination.BATTERY_SAVER -> vendorPages.filter { it.kind == VendorPageKind.BATTERY }.map(::vendorIntent)
        }
        return pages + Intent(Settings.ACTION_APPLICATION_DETAILS_SETTINGS, packageUri)
    }

    private fun batteryExempt() = context.getSystemService(PowerManager::class.java)
        ?.isIgnoringBatteryOptimizations(context.packageName) == true

    private fun vendorIntent(page: VendorBackgroundPage) = Intent().apply {
        component = ComponentName(page.packageName, page.activity)
        page.extras.forEach { (key, value) -> putExtra(key, value) }
    }
}
