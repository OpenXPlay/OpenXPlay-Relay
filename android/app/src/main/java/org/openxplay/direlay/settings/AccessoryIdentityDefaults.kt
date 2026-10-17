// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-17
package org.openxplay.direlay.settings

import android.content.Context
import android.content.SharedPreferences
import com.shilapi.xcertplay.AirPlayPersistence

/**
 * Names the iPhone shows for this accessory: manufacturer and model in its settings, and the OEM label
 * under the car-button icon in CarPlay. DiPlay falls back to its own defaults ("DiPlay", and "BYD" for
 * the label) when a value is missing or blank, and its settings menu saves those defaults as values.
 * The relay saves OpenXPlay values through DiPlay's persistence instead:
 *
 * - a missing or blank value always gets the OpenXPlay value, since DiPlay would show its default;
 * - a stored DiPlay default is replaced once, by the first start of a relay that has this migration;
 * - any other stored value, and a DiPlay default saved after that first start, is the user's choice.
 */
object AccessoryIdentityDefaults {
    const val MANUFACTURER = "OpenXPlay"
    const val MODEL = "OpenXPlay Relay"
    const val OEM_LABEL = "OpenXPlay"

    /** DiPlay's preference file and keys, read only to tell a missing value from a saved default. */
    private const val DIPLAY_PREFERENCES = "xcertplay_airplay"
    /** Relay preference marking that stored DiPlay defaults were replaced. */
    private const val MIGRATED = "identityDefaultsMigrated"

    private class Field(
        val key: String,
        val diplayDefault: String,
        val relayDefault: String,
        val save: (Context, String) -> Unit,
    )

    private val fields = listOf(
        Field("manufacturer", AirPlayPersistence.DEFAULT_MANUFACTURER, MANUFACTURER, AirPlayPersistence::saveManufacturer),
        Field("model", AirPlayPersistence.DEFAULT_MODEL, MODEL, AirPlayPersistence::saveModel),
        Field("oem_label", AirPlayPersistence.DEFAULT_OEM_LABEL, OEM_LABEL, AirPlayPersistence::saveOemLabel),
    )

    /** Idempotent; [relay] holds the one-time migration marker. */
    fun apply(context: Context, relay: SharedPreferences) {
        val identity = context.getSharedPreferences(DIPLAY_PREFERENCES, Context.MODE_PRIVATE)
        val migrate = !relay.getBoolean(MIGRATED, false)
        for (field in fields) {
            val stored = identity.getString(field.key, null)?.trim().orEmpty()
            if (stored.isEmpty() || (migrate && stored == field.diplayDefault)) {
                field.save(context, field.relayDefault)
            }
        }
        if (migrate) relay.edit().putBoolean(MIGRATED, true).apply()
    }
}
