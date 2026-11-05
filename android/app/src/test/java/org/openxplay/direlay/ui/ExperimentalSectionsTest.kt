// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-29
package org.openxplay.direlay.ui

import org.junit.Assert.*
import org.junit.Test
import org.openxplay.direlay.settings.ExperimentalOptions
import java.util.Locale

class ExperimentalSectionsTest {
    @Test fun sectionsAreNewestFirstAndTheirOptionsFollowReleaseOrder() {
        assertEquals(listOf("0.2.7", "0.2.0"), experimentalSections.map { it.version })
        assertEquals(listOf(listOf(ExperimentalOption.CARLINK_WIFI_RECOVERY),
            listOf(ExperimentalOption.SEND_BLUETOOTH_ADDRESS)), experimentalSections.map { it.options })
    }

    @Test fun everyLaboratoryOptionAppearsExactlyOnceAndPromotedChoicesStayOut() {
        val options = experimentalSections.flatMap { it.options }
        assertEquals(ExperimentalOption.entries.toSet(), options.toSet())
        assertEquals(options.size, options.toSet().size)
        val selected = ExperimentalOptions(sendBluetoothAddress = true, iapChannelWhenOffered = true, releaseBluetoothOnRequest = true)
        val names = selected.names().filter { it !in ExperimentalOptions.OFF.names() }
        assertEquals(selected.optionCount, options.size)
        assertEquals(names.toSet(), options.map { it.name.lowercase(Locale.ROOT) }.toSet())
        assertTrue(experimentalSections.all { it.options.isNotEmpty() })
    }
}
