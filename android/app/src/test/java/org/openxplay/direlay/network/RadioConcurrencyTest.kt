// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-29
package org.openxplay.direlay.network

import org.junit.Assert.*
import org.junit.Test
import org.openxplay.direlay.settings.WirelessNetworkChoice

class RadioConcurrencyTest {
    @Test fun theVivoSequenceIsExclusiveAtTheFirstHotspotStart() {
        val saved = mutableListOf<RadioObservation>()
        val radios = RadioConcurrencyObserver(platform = null, save = { saved += it })
        assertEquals(RadioConcurrency.UNKNOWN, radios.concurrency)
        assertNull(radios.observe(wifiOn = true, hotspotOn = false, now = 0, wall = 100))
        // 09:05:30: wifi_disconnected, wifi_disabled and hotspot_on within half a second.
        assertNull(radios.observe(wifiOn = false, hotspotOn = false, now = 500, wall = 600))
        val observed = radios.observe(wifiOn = false, hotspotOn = true, now = 1_000, wall = 1_100)
        assertEquals(RadioObservation(true, 1_100, "wifi_off_with_hotspot"), observed)
        assertEquals(RadioConcurrency.EXCLUSIVE, radios.concurrency)
        assertEquals(listOf(observed), saved)
        // 09:05:55: hotspot_off, then wifi_enabled 0.19 s later; the same conclusion is not reported again.
        assertNull(radios.observe(wifiOn = false, hotspotOn = false, now = 25_000, wall = 0))
        assertNull(radios.observe(wifiOn = true, hotspotOn = false, now = 25_500, wall = 0))
        assertEquals(1, saved.size)
    }

    @Test fun wifiComingBackRightAfterTheHotspotStopsIsExclusiveToo() {
        val radios = RadioConcurrencyObserver(platform = true)
        radios.observe(wifiOn = false, hotspotOn = true, now = 0, wall = 0)
        assertNull(radios.observe(wifiOn = false, hotspotOn = false, now = 1_000, wall = 0))
        assertEquals("wifi_back_after_hotspot", radios.observe(wifiOn = true, hotspotOn = false, now = 1_200, wall = 7)?.evidence)
        assertEquals(RadioConcurrency.EXCLUSIVE, radios.concurrency)
    }

    @Test fun bothOnForTheWindowMeansSupportedAndReplacesAnOlderExclusiveObservation() {
        val radios = RadioConcurrencyObserver(platform = false, observed = RadioObservation(true, 1, "wifi_off_with_hotspot"))
        assertEquals(RadioConcurrency.EXCLUSIVE, radios.concurrency)
        assertNull(radios.observe(wifiOn = true, hotspotOn = true, now = 0, wall = 0))
        assertNull(radios.observe(wifiOn = true, hotspotOn = true, now = 4_999, wall = 0))
        assertEquals(RadioObservation(false, 9, "both_on"), radios.observe(wifiOn = true, hotspotOn = true, now = 5_000, wall = 9))
        assertEquals(RadioConcurrency.SUPPORTED, radios.concurrency)
    }

    @Test fun aBriefOverlapOrSlowManualTogglesProveNothing() {
        val radios = RadioConcurrencyObserver(platform = null)
        radios.observe(wifiOn = true, hotspotOn = false, now = 0, wall = 0)
        // Some systems start the hotspot a moment before they stop Wi-Fi.
        assertNull(radios.observe(wifiOn = true, hotspotOn = true, now = 500, wall = 0))
        assertNotNull(radios.observe(wifiOn = false, hotspotOn = true, now = 1_000, wall = 0))
        val manual = RadioConcurrencyObserver(platform = null)
        manual.observe(wifiOn = true, hotspotOn = false, now = 0, wall = 0)
        assertNull(manual.observe(wifiOn = false, hotspotOn = false, now = 1_000, wall = 0))
        assertNull(manual.observe(wifiOn = false, hotspotOn = true, now = 60_000, wall = 0))
        assertNull(manual.observe(wifiOn = null, hotspotOn = true, now = 60_500, wall = 0))
        assertEquals(RadioConcurrency.UNKNOWN, manual.concurrency)
    }

    @Test fun thePlatformAnswerCoversTheTimeBeforeAnyObservation() {
        assertEquals(RadioConcurrency.SUPPORTED, RadioConcurrencyObserver(true).concurrency)
        assertEquals(RadioConcurrency.EXCLUSIVE, RadioConcurrencyObserver(false).concurrency)
        assertEquals(RadioConcurrency.SUPPORTED, RadioConcurrencyObserver(false, RadioObservation(false, 0, "both_on")).concurrency)
    }

    private val hotspotOnWifiOff = NetworkReadings(wifi = false, hotspot = true)
    private val wifiOnly = NetworkReadings(wifi = true, hotspot = false)

    @Test fun carlinkOnTheHotspotOffersTheCarBrowserThenUsb() {
        val conflict = radioConflict(RadioConcurrency.EXCLUSIVE, true, WirelessNetworkChoice.PHONE_HOTSPOT, carWifi = true, wifiOnly)
        assertEquals(RadioConflict(RadioConflictKind.CAR_WIFI_WITH_HOTSPOT, listOf(RadioChoice.BROWSER, RadioChoice.USB)), conflict)
        assertEquals("kind=car_wifi_with_hotspot choices=browser,usb", conflict?.line())
        assertNull(radioConflict(RadioConcurrency.SUPPORTED, true, WirelessNetworkChoice.PHONE_HOTSPOT, true, wifiOnly))
        assertNull(radioConflict(RadioConcurrency.UNKNOWN, true, WirelessNetworkChoice.PHONE_HOTSPOT, true, wifiOnly))
        assertNull(radioConflict(RadioConcurrency.EXCLUSIVE, false, WirelessNetworkChoice.PHONE_HOTSPOT, true, hotspotOnWifiOff))
    }

    @Test fun wifiDirectWithTheCarOnTheHotspotOffersTheHotspotFirst() {
        assertEquals(RadioConflict(RadioConflictKind.HOTSPOT_STOPS_WIFI,
            listOf(RadioChoice.PHONE_HOTSPOT, RadioChoice.SHARED_WIFI, RadioChoice.USB)),
            radioConflict(RadioConcurrency.EXCLUSIVE, true, WirelessNetworkChoice.WIFI_DIRECT, carWifi = false, hotspotOnWifiOff))
        assertEquals(listOf(RadioChoice.PHONE_HOTSPOT, RadioChoice.USB),
            radioConflict(RadioConcurrency.EXCLUSIVE, true, WirelessNetworkChoice.EXISTING_WIFI, false, hotspotOnWifiOff)?.choices)
        assertEquals(listOf(RadioChoice.USB),
            radioConflict(RadioConcurrency.EXCLUSIVE, true, WirelessNetworkChoice.EXISTING_WIFI, true, hotspotOnWifiOff)?.choices)
        // Wi-Fi on, or the hotspot alone serving the iPhone and the car: nothing to resolve.
        assertNull(radioConflict(RadioConcurrency.EXCLUSIVE, true, WirelessNetworkChoice.WIFI_DIRECT, false, wifiOnly))
        assertNull(radioConflict(RadioConcurrency.EXCLUSIVE, true, WirelessNetworkChoice.PHONE_HOTSPOT, false, hotspotOnWifiOff))
        assertNull(radioConflict(RadioConcurrency.EXCLUSIVE, true, WirelessNetworkChoice.WIFI_DIRECT, false, NetworkReadings()))
    }
}
