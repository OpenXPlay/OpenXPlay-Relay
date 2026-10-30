// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-29
package org.openxplay.direlay.background

import org.junit.Assert.*
import org.junit.Test
import org.openxplay.direlay.background.VendorPageKind.*
import java.util.Locale

class VendorBackgroundCatalogueTest {
    private val packageName = "org.openxplay.relay"
    private val appLabel = "OpenXPlay Relay"
    private fun candidates(manufacturer: String, brand: String = "unknown") =
        VendorBackgroundCatalogue.candidates(manufacturer, brand, packageName, appLabel)
    private val huawei = listOf(
        VendorBackgroundPage(AUTOSTART, "com.huawei.systemmanager", "com.huawei.systemmanager.startupmgr.ui.StartupNormalAppListActivity"),
        VendorBackgroundPage(AUTOSTART, "com.huawei.systemmanager", "com.huawei.systemmanager.optimize.process.ProtectActivity"))

    @Test fun everyMakerAndBrandAliasKeepsItsOrderedCandidatesAndExtras() {
        val families = listOf(
            listOf("xiaomi", "redmi", "poco") to listOf(
                VendorBackgroundPage(AUTOSTART, "com.miui.securitycenter", "com.miui.permcenter.autostart.AutoStartManagementActivity"),
                VendorBackgroundPage(BATTERY, "com.miui.powerkeeper", "com.miui.powerkeeper.ui.HiddenAppsConfigActivity",
                    mapOf("package_name" to packageName, "package_label" to appLabel))),
            listOf("huawei") to huawei,
            listOf("honor") to listOf(VendorBackgroundPage(AUTOSTART, "com.hihonor.systemmanager",
                "com.hihonor.systemmanager.startupmgr.ui.StartupNormalAppListActivity")) + huawei,
            listOf("oppo", "realme", "oneplus") to listOf(
                VendorBackgroundPage(AUTOSTART, "com.coloros.safecenter", "com.coloros.safecenter.permission.startup.StartupAppListActivity"),
                VendorBackgroundPage(AUTOSTART, "com.coloros.safecenter", "com.coloros.safecenter.startupapp.StartupAppListActivity"),
                VendorBackgroundPage(AUTOSTART, "com.oplus.safecenter", "com.oplus.safecenter.permission.startup.StartupAppListActivity"),
                VendorBackgroundPage(BATTERY, "com.oplus.battery", "com.oplus.powermanager.fuelgaue.PowerUsageModelActivity"),
                VendorBackgroundPage(BATTERY, "com.coloros.oppoguardelf", "com.coloros.powermanager.fuelgaue.PowerUsageModelActivity")),
            listOf("vivo", "iqoo") to listOf(
                VendorBackgroundPage(AUTOSTART, "com.vivo.permissionmanager", "com.vivo.permissionmanager.activity.BgStartUpManagerActivity"),
                VendorBackgroundPage(AUTOSTART, "com.iqoo.secure", "com.iqoo.secure.ui.phoneoptimize.BgStartUpManager"),
                VendorBackgroundPage(BATTERY, "com.iqoo.powersaving", "com.iqoo.powersaving.PowerSavingManagerActivity")),
            listOf("samsung") to listOf(
                VendorBackgroundPage(BATTERY, "com.samsung.android.lool", "com.samsung.android.sm.battery.ui.BatteryActivity"),
                VendorBackgroundPage(BATTERY, "com.samsung.android.sm", "com.samsung.android.sm.ui.battery.BatteryActivity")),
            listOf("meizu") to listOf(VendorBackgroundPage(AUTOSTART, "com.meizu.safe", "com.meizu.safe.permission.SmartBGActivity")),
            listOf("asus") to listOf(VendorBackgroundPage(AUTOSTART, "com.asus.mobilemanager", "com.asus.mobilemanager.autostart.AutoStartActivity")))
        families.forEach { (aliases, expected) -> aliases.forEach { alias ->
            for (name in listOf(alias, alias.uppercase(Locale.ROOT))) {
                assertEquals(name, expected, candidates(name))
                assertEquals(name, expected, candidates("unknown", name))
            }
        } }
    }

    @Test fun unknownPhonesHaveNoPagesAndRecognizedBrandsTakePrecedence() {
        assertTrue(candidates("Google", "Pixel").isEmpty())
        assertTrue(candidates("", "").isEmpty())
        assertEquals(candidates("honor"), candidates("HUAWEI", "Honor"))
        val battery = VendorBackgroundCatalogue.candidates("Xiaomi", "Redmi", "test.package", "Test Relay").single { it.kind == BATTERY }
        assertEquals(mapOf("package_name" to "test.package", "package_label" to "Test Relay"), battery.extras)
    }
}
