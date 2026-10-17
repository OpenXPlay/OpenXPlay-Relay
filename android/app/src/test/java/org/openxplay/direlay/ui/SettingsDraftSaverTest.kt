// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-15
package org.openxplay.direlay.ui

import android.app.Application
import android.os.Bundle
import androidx.compose.runtime.saveable.SaverScope
import com.shilapi.xcertplay.orchestration.MfiTarget
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.openxplay.direlay.settings.*

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [33], manifest = Config.NONE, application = Application::class)
class SettingsDraftSaverTest {
    private val base = RelaySettings(ConnectionMode.WIRELESS, MfiTarget.LOCAL, "", "", "", null,
        30, false, true, false, null)
    private val scope = object : SaverScope { override fun canBeSaved(value: Any) = true }
    private fun bundle(value: RelaySettings): Bundle = with(relaySettingsSaver(base)) { requireNotNull(scope.save(value)) }
    @Test fun everyNetworkChoiceAndPairingFlagRoundTripWithCredentials() {
        for (choice in WirelessNetworkChoice.entries) for (required in listOf(false, true)) {
            val value = base.copy(iphoneAddress = if (required) "00:11:22:33:A9:30" else null, requirePairingCode = required, wirelessNetwork = WirelessNetworkSettings(
                choice = choice, manualSsid = "Phone", manualPassphrase = "12345678",
                sharedWifiNetworks = listOf(SharedWifiNetwork("Router", "abcdefgh"), SharedWifiNetwork("Car", ""))))
            assertEquals(value, relaySettingsSaver(base).restore(bundle(value)))
        }
    }
    @Test fun oldCarHotspotNameAndBundlesWithoutPairingFlagMigrate() {
        val bundle = bundle(base)
        val text = requireNotNull(bundle.getStringArrayList("text"))
        text[8] = "CAR_HOTSPOT"
        bundle.putStringArrayList("text", text)
        bundle.putBooleanArray("flags", requireNotNull(bundle.getBooleanArray("flags")).copyOf(7))
        val restored = requireNotNull(relaySettingsSaver(base).restore(bundle))
        assertEquals(WirelessNetworkChoice.PHONE_HOTSPOT, restored.wirelessNetwork.choice)
        assertFalse(restored.requirePairingCode)
    }
    @Test fun oldSingleNetworkDraftMigrates() {
        val bundle = bundle(base)
        val text = requireNotNull(bundle.getStringArrayList("text"))
        text[13] = "Legacy"; text.add("legacy-password")
        bundle.putStringArrayList("text", text)
        assertEquals(listOf(SharedWifiNetwork("Legacy", "legacy-password")),
            requireNotNull(relaySettingsSaver(base).restore(bundle)).wirelessNetwork.sharedWifiNetworks)
    }
}
