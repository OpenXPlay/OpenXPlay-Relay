// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-14
package org.openxplay.direlay.settings

import android.Manifest
import com.shilapi.xcertplay.orchestration.WirelessHotspotMode

/** Matches DiPlay's WiredPermissions and WirelessPermissions, plus microphone and notifications. */
object StartupPermissions {
    fun required(sdk: Int, wireless: Boolean, hotspot: WirelessHotspotMode, locationReporting: Boolean, carlink: Boolean = false): List<String> =
        buildList {
            add(Manifest.permission.RECORD_AUDIO)
            if (sdk >= 33) add(Manifest.permission.POST_NOTIFICATIONS)
            if (sdk >= 37) add("android.permission.ACCESS_LOCAL_NETWORK")
            if (wireless) {
                if (sdk >= 31) add(Manifest.permission.BLUETOOTH_CONNECT)
                if (hotspot != WirelessHotspotMode.EXISTING_WIFI) {
                    if (sdk >= 33) add(Manifest.permission.NEARBY_WIFI_DEVICES)
                    else {
                        add(Manifest.permission.ACCESS_COARSE_LOCATION)
                        add(Manifest.permission.ACCESS_FINE_LOCATION)
                    }
                }
            }
            addAll(carlink(sdk, carlink))
            if (locationReporting) {
                add(Manifest.permission.ACCESS_COARSE_LOCATION)
                add(Manifest.permission.ACCESS_FINE_LOCATION)
            }
        }.distinct()

    fun carlink(sdk: Int, enabled: Boolean): List<String> = if (!enabled) emptyList() else buildList {
        if (sdk >= 31) {
            add(Manifest.permission.BLUETOOTH_SCAN)
            add(Manifest.permission.BLUETOOTH_CONNECT)
        }
        if (sdk >= 33) add(Manifest.permission.NEARBY_WIFI_DEVICES)
        else add(Manifest.permission.ACCESS_FINE_LOCATION)
        if (sdk >= 37) add("android.permission.ACCESS_LOCAL_NETWORK")
    }
}
