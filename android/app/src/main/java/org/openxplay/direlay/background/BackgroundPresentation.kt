// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-29
package org.openxplay.direlay.background

data class BackgroundStatus(val batteryExempt: Boolean, val backgroundRestricted: Boolean,
    val vendorPages: Set<VendorPageKind> = emptySet())
enum class BackgroundSummary { ALLOWED, LIMITED }
enum class BackgroundDestination { BATTERY_OPTIMIZATION, APP_DETAILS, AUTOSTART, BATTERY_SAVER }

data class BackgroundPresentation(val summary: BackgroundSummary, val batteryAllowed: Boolean,
    val showRestriction: Boolean, val vendorRows: List<VendorPageKind>) {
    val showBatteryRequest get() = !batteryAllowed
    val showVendorGroup get() = vendorRows.isNotEmpty()
}

fun presentBackground(status: BackgroundStatus) = BackgroundPresentation(
    if (status.batteryExempt && !status.backgroundRestricted) BackgroundSummary.ALLOWED else BackgroundSummary.LIMITED,
    status.batteryExempt, status.backgroundRestricted, VendorPageKind.entries.filter { it in status.vendorPages })
