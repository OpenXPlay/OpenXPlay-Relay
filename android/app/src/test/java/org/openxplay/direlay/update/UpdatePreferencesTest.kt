// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-25
package org.openxplay.direlay.update

import android.app.Application
import android.content.Context
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [33], application = Application::class)
class UpdatePreferencesTest {
    @Test fun stableIsTheDefaultAndBetaPersistsOnlyInTheUpdatesPreferenceFile() {
        val context = RuntimeEnvironment.getApplication()
        val preferences = context.getSharedPreferences("updates", Context.MODE_PRIVATE)
        preferences.edit().clear().commit()
        val store = UpdatePreferences(context)
        assertEquals(UpdateChannel.STABLE, store.channel)
        store.channel = UpdateChannel.BETA
        assertEquals("beta", preferences.getString("channel", null))
        assertEquals(UpdateChannel.BETA, UpdatePreferences(context).channel)
        assertEquals(setOf("channel"), preferences.all.keys)
        store.channel = UpdateChannel.STABLE
        assertEquals("stable", preferences.getString("channel", null))
    }
    @Test fun unknownMissingAndWrongTypeValuesFallBackToStable() {
        val preferences = RuntimeEnvironment.getApplication().getSharedPreferences("updates", Context.MODE_PRIVATE)
        preferences.edit().clear().commit()
        assertEquals(UpdateChannel.STABLE, UpdatePreferences(preferences).channel)
        for (value in listOf("unknown", "BETA", "")) {
            preferences.edit().putString("channel", value).commit()
            assertEquals(UpdateChannel.STABLE, UpdatePreferences(preferences).channel)
        }
        preferences.edit().putInt("channel", 1).commit()
        assertEquals(UpdateChannel.STABLE, UpdatePreferences(preferences).channel)
    }
}
