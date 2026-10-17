// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-17
package org.openxplay.direlay.settings

import android.app.Application
import com.shilapi.xcertplay.AirPlayPersistence
import org.junit.Assert.*
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [33], manifest = Config.NONE, application = Application::class)
class AccessoryIdentityDefaultsTest {
    private val context get() = RuntimeEnvironment.getApplication()
    private val relay get() = context.getSharedPreferences("direlay", 0)
    private fun apply() = AccessoryIdentityDefaults.apply(context, relay)
    private fun identity() = listOf(
        AirPlayPersistence.loadManufacturer(context),
        AirPlayPersistence.loadModel(context),
        AirPlayPersistence.loadOemLabel(context),
    )
    private fun save(manufacturer: String, model: String, oemLabel: String) {
        AirPlayPersistence.saveManufacturer(context, manufacturer)
        AirPlayPersistence.saveModel(context, model)
        AirPlayPersistence.saveOemLabel(context, oemLabel)
    }
    private val openXPlay = listOf("OpenXPlay", "OpenXPlay Relay", "OpenXPlay")

    @Before fun clear() {
        for (name in listOf("direlay", "diplay", "xcertplay_airplay")) {
            context.getSharedPreferences(name, 0).edit().clear().commit()
        }
    }

    @Test fun freshInstallSavesOpenXPlayValues() {
        assertEquals(listOf("DiPlay", "DiPlay", "BYD"), identity())
        apply()
        assertEquals(openXPlay, identity())
        val stored = context.getSharedPreferences("xcertplay_airplay", 0)
        assertEquals("OpenXPlay", stored.getString("oem_label", null))
    }

    @Test fun savedDiplayDefaultsAreReplacedOnceThenKeptAsChoices() {
        // DiPlay's settings menu saves its defaults with every other setting.
        save("DiPlay", "DiPlay", "BYD")
        apply()
        assertEquals(openXPlay, identity())
        // Chosen after the migration: the user's own values, even when they equal DiPlay's defaults.
        save("DiPlay", "DiPlay", "BYD")
        apply()
        assertEquals(listOf("DiPlay", "DiPlay", "BYD"), identity())
    }

    @Test fun userValuesAreNeverOverwritten() {
        save("Acme", "Head unit", "My car")
        apply()
        apply()
        assertEquals(listOf("Acme", "Head unit", "My car"), identity())
    }

    @Test fun eachFieldMigratesIndependently() {
        AirPlayPersistence.saveManufacturer(context, "Acme")
        AirPlayPersistence.saveOemLabel(context, " BYD ")
        apply()
        assertEquals(listOf("Acme", "OpenXPlay Relay", "OpenXPlay"), identity())
    }

    @Test fun blankValuesAlwaysGetOpenXPlayValuesInsteadOfDiplayDefaults() {
        apply()
        save(" ", "", "  ")
        apply()
        assertEquals(openXPlay, identity())
    }

    @Test fun relaySettingsLoadAppliesTheDefaults() {
        AirPlayPersistence.saveOemLabel(context, "BYD")
        RelaySettingsStore(context).load()
        assertEquals(openXPlay, identity())
    }
}
