// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-21
package org.openxplay.direlay.settings

import android.content.Context
import android.content.SharedPreferences
import com.shilapi.xcertplay.AirPlayPersistence

/** Enable car Bluetooth audio once, including on upgrades; subsequent values belong to the user. */
internal object CarBluetoothAudioDefaults {
    @Synchronized fun apply(context: Context, relay: SharedPreferences) {
        if (relay.contains("carBluetoothAudioDefaulted")) return
        AirPlayPersistence.saveCarBluetoothAudio(context, true)
        relay.edit().putBoolean("carBluetoothAudioDefaulted", true).apply()
    }
}
