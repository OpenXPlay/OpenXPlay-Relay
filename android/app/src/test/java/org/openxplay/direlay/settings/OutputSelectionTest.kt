// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-25
package org.openxplay.direlay.settings

import com.shilapi.xcertplay.orchestration.MfiTarget
import org.junit.Assert.*
import org.junit.Test

class OutputSelectionTest {
    private val base = RelaySettings(ConnectionMode.WIRELESS, MfiTarget.LOCAL, "", "", "", null,
        autoStartOnBoot = false, autoStartWhenReady = true, localPreview = false, webPort = null)

    @Test fun singleChoicesAndWirelessDefaultsDoNotCollapseCombinations() {
        val choices = listOf(OutputChoice.BROWSER, OutputChoice.CARLINK, OutputChoice.WIRELESS, OutputChoice.WIRELESS)
        RelayOutput.entries.forEachIndexed { index, output ->
            assertEquals(choices[index], OutputSelection.choice(setOf(output)))
            assertEquals(setOf(output), OutputSelection.outputs(choices[index],
                if (output == RelayOutput.MIRACAST) WirelessDisplayKind.MIRACAST else WirelessDisplayKind.DLNA))
        }
        assertEquals(setOf(RelayOutput.DLNA), OutputSelection.outputs(OutputChoice.WIRELESS))
        assertNull(OutputSelection.choice(emptySet()))
        for (mask in 0..15) {
            val outputs = RelayOutput.entries.filterIndexed { index, _ -> mask and (1 shl index) != 0 }.toSet()
            if (outputs.size > 1) assertNull(OutputSelection.choice(outputs))
            assertEquals(if (RelayOutput.MIRACAST in outputs && RelayOutput.DLNA !in outputs)
                WirelessDisplayKind.MIRACAST else WirelessDisplayKind.DLNA, OutputSelection.wirelessKind(outputs))
        }
    }

    @Test fun allSixteenLegacyCombinationsKeepExactlyTheOutputsThatRan() {
        val oldPairs = listOf(setOf(RelayOutput.BROWSER), setOf(RelayOutput.CARLINK),
            setOf(RelayOutput.BROWSER, RelayOutput.CARLINK), setOf(RelayOutput.BROWSER, RelayOutput.CARLINK))
        for (mask in 0..15) {
            val expected = oldPairs[mask and 3] +
                (if (mask and 4 != 0) setOf(RelayOutput.DLNA) else emptySet()) +
                (if (mask and 8 != 0) setOf(RelayOutput.MIRACAST) else emptySet())
            assertEquals("legacy mask=$mask", expected,
                OutputSelection.legacy(mask and 1 != 0, mask and 2 != 0, mask and 4 != 0, mask and 8 != 0))
        }
    }

    @Test fun choosingReplacesAllOutputsAndOnlyRadioUsersMoveWirelessCarPlayToHotspot() {
        for (output in RelayOutput.entries) for (wireless in listOf(false, true)) {
            val choice = OutputSelection.choice(setOf(output))!!
            val kind = OutputSelection.wirelessKind(setOf(output))
            val after = OutputSelection.choose(base, choice, kind, wireless)
            assertEquals(setOf(output), after.outputs)
            assertEquals(if (wireless && output in setOf(RelayOutput.CARLINK, RelayOutput.MIRACAST))
                WirelessNetworkChoice.PHONE_HOTSPOT else WirelessNetworkChoice.WIFI_DIRECT, after.wirelessNetwork.choice)
            val combined = base.copy(outputs = RelayOutput.entries.toSet(), connectionMode = ConnectionMode.USB)
            assertEquals(setOf(output), OutputSelection.choose(combined, choice, kind, false).outputs)
        }
    }

    @Test fun leavingEitherRadioUserResetsOnlyAnUnnamedHotspot() {
        for (output in listOf(RelayOutput.CARLINK, RelayOutput.MIRACAST)) {
            val radio = OutputSelection.choose(base, OutputSelection.choice(setOf(output))!!,
                OutputSelection.wirelessKind(setOf(output)), true)
            for (choice in listOf(OutputChoice.BROWSER, OutputChoice.WIRELESS)) {
                val after = OutputSelection.choose(radio, choice, WirelessDisplayKind.DLNA, true)
                assertEquals(WirelessNetworkChoice.WIFI_DIRECT, after.wirelessNetwork.choice)
                val named = radio.copy(wirelessNetwork = radio.wirelessNetwork.copy(manualSsid = "Car"))
                assertEquals(named.wirelessNetwork, OutputSelection.choose(named, choice, WirelessDisplayKind.DLNA, true).wirelessNetwork)
            }
        }
    }

    @Test fun developerCannotRemoveTheLastOutputAndConfirmationUsesTheNetworkPolicy() {
        for (only in RelayOutput.entries) {
            val before = base.copy(outputs = setOf(only))
            assertNull(OutputSelection.toggle(before, only, false, true))
            for (output in RelayOutput.entries) {
                val after = OutputSelection.toggle(before, output, true, true)!!
                assertEquals(setOf(only, output), after.outputs)
                val candidate = before.copy(outputs = after.outputs)
                assertEquals(CarlinkConnectionPolicy.enabling(before, candidate, true), after)
                val confirmation = output != only && output in setOf(RelayOutput.CARLINK, RelayOutput.MIRACAST)
                assertEquals(confirmation, after != candidate)
                if (output != only) {
                    val removed = OutputSelection.toggle(after, output, false, false)!!
                    assertEquals(before.outputs, removed.outputs)
                    assertEquals(after.wirelessNetwork, removed.wirelessNetwork)
                }
            }
        }
    }
}
