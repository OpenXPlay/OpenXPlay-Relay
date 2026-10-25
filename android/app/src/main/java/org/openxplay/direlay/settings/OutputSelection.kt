// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-25
package org.openxplay.direlay.settings

enum class RelayOutput(val wire: String) { BROWSER("web"), CARLINK("carlink"), DLNA("dlna"), MIRACAST("miracast") }
enum class OutputChoice { BROWSER, CARLINK, WIRELESS }
enum class WirelessDisplayKind(val output: RelayOutput) { DLNA(RelayOutput.DLNA), MIRACAST(RelayOutput.MIRACAST) }

object OutputSelection {
    fun choice(outputs: Set<RelayOutput>): OutputChoice? = when (outputs.singleOrNull()) {
        RelayOutput.BROWSER -> OutputChoice.BROWSER
        RelayOutput.CARLINK -> OutputChoice.CARLINK
        RelayOutput.DLNA, RelayOutput.MIRACAST -> OutputChoice.WIRELESS
        null -> null
    }
    fun wirelessKind(outputs: Set<RelayOutput>) =
        if (RelayOutput.MIRACAST in outputs && RelayOutput.DLNA !in outputs) WirelessDisplayKind.MIRACAST else WirelessDisplayKind.DLNA
    fun outputs(choice: OutputChoice, kind: WirelessDisplayKind = WirelessDisplayKind.DLNA): Set<RelayOutput> = setOf(when (choice) {
        OutputChoice.BROWSER -> RelayOutput.BROWSER
        OutputChoice.CARLINK -> RelayOutput.CARLINK
        OutputChoice.WIRELESS -> kind.output
    })
    fun choose(settings: RelaySettings, choice: OutputChoice, kind: WirelessDisplayKind, wireless: Boolean): RelaySettings {
        val chosen = CarlinkConnectionPolicy.enabling(settings, settings.copy(outputs = outputs(choice, kind)), wireless)
        val network = chosen.wirelessNetwork
        return if (!CarlinkConnectionPolicy.usesWifiDirect(chosen) && network.choice == WirelessNetworkChoice.PHONE_HOTSPOT &&
            network.manualSsid.isEmpty()) chosen.copy(wirelessNetwork = network.copy(choice = WirelessNetworkChoice.WIFI_DIRECT)) else chosen
    }

    fun toggle(settings: RelaySettings, output: RelayOutput, on: Boolean, wireless: Boolean): RelaySettings? {
        val outputs = if (on) settings.outputs + output else settings.outputs - output
        return outputs.takeIf { it.isNotEmpty() }?.let {
            CarlinkConnectionPolicy.enabling(settings, settings.copy(outputs = it), wireless)
        }
    }
    fun legacy(carlinkOutput: Boolean, bothOutputs: Boolean, dlnaOutput: Boolean, miracastOutput: Boolean): Set<RelayOutput> = buildSet {
        if (!carlinkOutput || bothOutputs) add(RelayOutput.BROWSER)
        if (carlinkOutput || bothOutputs) add(RelayOutput.CARLINK)
        if (dlnaOutput) add(RelayOutput.DLNA)
        if (miracastOutput) add(RelayOutput.MIRACAST)
    }
}
