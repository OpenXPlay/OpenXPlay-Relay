// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-29
package org.openxplay.direlay.background

import java.util.Locale

enum class VendorPageKind { AUTOSTART, BATTERY }
data class VendorBackgroundPage(val kind: VendorPageKind, val packageName: String, val activity: String,
    val extras: Map<String, String> = emptyMap())

object VendorBackgroundCatalogue {
    fun candidates(manufacturer: String, brand: String, packageName: String, appLabel: String): List<VendorBackgroundPage> =
        sequenceOf(brand, manufacturer).map { it.trim().lowercase(Locale.ROOT) }
            .mapNotNull { pages(it, packageName, appLabel) }.firstOrNull().orEmpty()

    private fun pages(maker: String, packageName: String, appLabel: String): List<VendorBackgroundPage>? = when (maker) {
        "xiaomi", "redmi", "poco" -> listOf(
            VendorBackgroundPage(VendorPageKind.AUTOSTART, "com.miui.securitycenter",
                "com.miui.permcenter.autostart.AutoStartManagementActivity"),
            VendorBackgroundPage(VendorPageKind.BATTERY, "com.miui.powerkeeper",
                "com.miui.powerkeeper.ui.HiddenAppsConfigActivity", mapOf("package_name" to packageName, "package_label" to appLabel)))
        "huawei" -> huawei
        "honor" -> listOf(VendorBackgroundPage(VendorPageKind.AUTOSTART, "com.hihonor.systemmanager",
            "com.hihonor.systemmanager.startupmgr.ui.StartupNormalAppListActivity")) + huawei
        "oppo", "realme", "oneplus" -> listOf(
            VendorBackgroundPage(VendorPageKind.AUTOSTART, "com.coloros.safecenter",
                "com.coloros.safecenter.permission.startup.StartupAppListActivity"),
            VendorBackgroundPage(VendorPageKind.AUTOSTART, "com.coloros.safecenter",
                "com.coloros.safecenter.startupapp.StartupAppListActivity"),
            VendorBackgroundPage(VendorPageKind.AUTOSTART, "com.oplus.safecenter",
                "com.oplus.safecenter.permission.startup.StartupAppListActivity"),
            VendorBackgroundPage(VendorPageKind.BATTERY, "com.oplus.battery",
                "com.oplus.powermanager.fuelgaue.PowerUsageModelActivity"),
            VendorBackgroundPage(VendorPageKind.BATTERY, "com.coloros.oppoguardelf",
                "com.coloros.powermanager.fuelgaue.PowerUsageModelActivity"))
        "vivo", "iqoo" -> listOf(
            VendorBackgroundPage(VendorPageKind.AUTOSTART, "com.vivo.permissionmanager",
                "com.vivo.permissionmanager.activity.BgStartUpManagerActivity"),
            VendorBackgroundPage(VendorPageKind.AUTOSTART, "com.iqoo.secure",
                "com.iqoo.secure.ui.phoneoptimize.BgStartUpManager"),
            VendorBackgroundPage(VendorPageKind.BATTERY, "com.iqoo.powersaving",
                "com.iqoo.powersaving.PowerSavingManagerActivity"))
        "samsung" -> listOf(
            VendorBackgroundPage(VendorPageKind.BATTERY, "com.samsung.android.lool",
                "com.samsung.android.sm.battery.ui.BatteryActivity"),
            VendorBackgroundPage(VendorPageKind.BATTERY, "com.samsung.android.sm",
                "com.samsung.android.sm.ui.battery.BatteryActivity"))
        "meizu" -> listOf(VendorBackgroundPage(VendorPageKind.AUTOSTART, "com.meizu.safe",
            "com.meizu.safe.permission.SmartBGActivity"))
        "asus" -> listOf(VendorBackgroundPage(VendorPageKind.AUTOSTART, "com.asus.mobilemanager",
            "com.asus.mobilemanager.autostart.AutoStartActivity"))
        else -> null
    }

    private val huawei = listOf(
        VendorBackgroundPage(VendorPageKind.AUTOSTART, "com.huawei.systemmanager",
            "com.huawei.systemmanager.startupmgr.ui.StartupNormalAppListActivity"),
        VendorBackgroundPage(VendorPageKind.AUTOSTART, "com.huawei.systemmanager",
            "com.huawei.systemmanager.optimize.process.ProtectActivity"))
}
