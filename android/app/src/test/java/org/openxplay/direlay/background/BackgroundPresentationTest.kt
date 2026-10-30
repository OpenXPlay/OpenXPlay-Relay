// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-29
package org.openxplay.direlay.background

import org.junit.Assert.*
import org.junit.Test
import org.openxplay.direlay.background.VendorPageKind.*

class BackgroundPresentationTest {
    @Test fun exemptionAndRestrictionSetTheSummaryAndSystemRowsIndependently() {
        for (exempt in listOf(false, true)) for (restricted in listOf(false, true)) {
            val view = presentBackground(BackgroundStatus(exempt, restricted))
            assertEquals(if (exempt && !restricted) BackgroundSummary.ALLOWED else BackgroundSummary.LIMITED, view.summary)
            assertEquals(exempt, view.batteryAllowed)
            assertEquals(!exempt, view.showBatteryRequest)
            assertEquals(restricted, view.showRestriction)
            assertFalse(view.showVendorGroup)
            assertTrue(view.vendorRows.isEmpty())
        }
    }

    @Test fun onlyResolvedVendorKindsAppearInAutostartThenBatteryOrder() {
        for (pages in listOf(emptySet(), setOf(AUTOSTART), setOf(BATTERY), linkedSetOf(BATTERY, AUTOSTART))) {
            val view = presentBackground(BackgroundStatus(true, false, pages))
            assertEquals(pages.isNotEmpty(), view.showVendorGroup)
            assertEquals(listOf(AUTOSTART, BATTERY).filter { it in pages }, view.vendorRows)
            assertEquals(BackgroundSummary.ALLOWED, view.summary)
        }
    }
}
