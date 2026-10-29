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
        assertEquals(listOf("0.2.3", "0.2.2", "0.2.1", "0.2.0"), experimentalSections.map { it.version })
        assertEquals(listOf(
            listOf(ExperimentalOption.IAP_CHANNEL_WHEN_OFFERED, ExperimentalOption.RELEASE_BLUETOOTH_ON_REQUEST,
                ExperimentalOption.AWAIT_IPHONE_RECONNECT, ExperimentalOption.RADIO_CONFLICT_PROMPT, ExperimentalOption.STALL_WATCHDOG),
            listOf(ExperimentalOption.SCREEN_SESSION_ISOLATION, ExperimentalOption.SCREEN_RECONNECT_RESET,
                ExperimentalOption.SCREEN_DELIVERY_SERIALIZATION),
            listOf(ExperimentalOption.REMOTE_PLAYBACK_PROOF, ExperimentalOption.CANCEL_RECOVERED_RETRIES,
                ExperimentalOption.STARTUP_STATUS, ExperimentalOption.STARTUP_DIAGNOSTICS, ExperimentalOption.NETWORK_OBSERVATION_BACKOFF),
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
