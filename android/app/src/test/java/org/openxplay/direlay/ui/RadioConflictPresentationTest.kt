// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-29
package org.openxplay.direlay.ui

import com.shilapi.xcertplay.orchestration.MfiTarget
import org.junit.Assert.*
import org.junit.Test
import org.openxplay.direlay.R
import org.openxplay.direlay.network.RadioChoice
import org.openxplay.direlay.settings.*
import org.openxplay.stream.core.FrameRatePreference

class RadioConflictPresentationTest {
    private val browser = RelaySettings(ConnectionMode.WIRELESS, MfiTarget.LOCAL, "", "", "", null,
        FrameRatePreference.AUTO, false, true, false, null, wirelessNetwork = WirelessNetworkSettings(choice = WirelessNetworkChoice.WIFI_DIRECT))

    @Test fun eachChoiceLandsOnItsSettingsPageWithTheChangeInTheDraft() {
        val hotspot = radioChoiceSettings(browser, RadioChoice.PHONE_HOTSPOT)
        assertEquals(WirelessNetworkChoice.PHONE_HOTSPOT, hotspot.wirelessNetwork.choice)
        assertEquals(browser.copy(wirelessNetwork = hotspot.wirelessNetwork), hotspot)
        assertEquals(WirelessNetworkChoice.EXISTING_WIFI, radioChoiceSettings(browser, RadioChoice.SHARED_WIFI).wirelessNetwork.choice)
        val carlink = browser.copy(outputs = setOf(RelayOutput.CARLINK),
            wirelessNetwork = browser.wirelessNetwork.copy(choice = WirelessNetworkChoice.PHONE_HOTSPOT))
        val browserOutput = radioChoiceSettings(carlink, RadioChoice.BROWSER)
        assertEquals(carlink.copy(outputs = setOf(RelayOutput.BROWSER)), browserOutput)
        assertEquals("settings/output", relayPageKey(radioChoiceRoute(RadioChoice.BROWSER)))
        val usb = radioChoiceSettings(carlink, RadioChoice.USB)
        assertEquals(ConnectionMode.USB, usb.connectionMode)
        assertEquals(carlink.wirelessNetwork, usb.wirelessNetwork)
        assertEquals("settings/connection/network", relayPageKey(radioChoiceRoute(RadioChoice.PHONE_HOTSPOT)))
        assertEquals("PHONE_HOTSPOT", radioChoiceRoute(RadioChoice.PHONE_HOTSPOT).substringAfter('#'))
        assertEquals("sharedWifi", radioChoiceRoute(RadioChoice.SHARED_WIFI).substringAfter('#'))
        assertEquals("settings/connection", relayPageKey(radioChoiceRoute(RadioChoice.USB)))
    }

    @Test fun choicesReuseTheSettingsWording() {
        assertEquals(RadioOption(R.string.direlay_car_hotspot, R.string.option_hotspot_hint), radioChoiceOption(RadioChoice.PHONE_HOTSPOT))
        assertEquals(RadioOption(R.string.direlay_existing_wifi, R.string.option_wifi_hint), radioChoiceOption(RadioChoice.SHARED_WIFI))
        assertEquals(RadioOption(R.string.ui_browser, R.string.option_browser_hint), radioChoiceOption(RadioChoice.BROWSER))
        assertEquals(RadioOption(R.string.ui_usb, R.string.option_usb_hint), radioChoiceOption(RadioChoice.USB))
    }
}
