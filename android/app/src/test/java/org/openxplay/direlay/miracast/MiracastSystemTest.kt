// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-22
package org.openxplay.direlay.miracast

import android.app.Application
import android.content.ActivityNotFoundException
import android.content.Context
import android.content.ContextWrapper
import android.content.Intent
import android.provider.Settings
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config
import org.robolectric.shadows.ShadowSettings

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [33], application = Application::class)
class MiracastSystemTest {
    private val context = RuntimeEnvironment.getApplication()
    private val actions = listOf(Settings.ACTION_CAST_SETTINGS, "android.settings.WIFI_DISPLAY_SETTINGS", Settings.ACTION_WIRELESS_SETTINGS)
    private class SettingsContext(context: Context) : ContextWrapper(context) {
        val available = mutableSetOf<String>()
        val denied = mutableSetOf<String>()
        val attempts = mutableListOf<Intent>()
        override fun startActivity(intent: Intent) {
            attempts += intent
            if (intent.action in denied) throw SecurityException()
            if (intent.action !in available) throw ActivityNotFoundException()
        }
    }
    @Test fun overlayPermissionPageNamesThisPackageAndCanOpenFromAService() {
        val intent = MiracastSystem.overlayPermissionIntent(context)
        assertEquals(Settings.ACTION_MANAGE_OVERLAY_PERMISSION, intent.action)
        assertEquals("package:${context.packageName}", intent.data.toString())
        assertTrue(intent.flags and Intent.FLAG_ACTIVITY_NEW_TASK != 0)
    }
    @Test fun overlayPermissionReadsTheCurrentSystemGrant() {
        ShadowSettings.setCanDrawOverlays(false)
        assertFalse(MiracastSystem.canDrawOverlays(context))
        ShadowSettings.setCanDrawOverlays(true)
        assertTrue(MiracastSystem.canDrawOverlays(context))
    }
    @Test fun castSettingsHavePriorityEvenWhenPackageVisibilityHidesTheActivity() {
        val hidden = SettingsContext(context).apply { available += actions }
        assertNull(Intent(actions[0]).resolveActivity(hidden.packageManager))
        assertTrue(MiracastSystem.openCastSettings(hidden))
        val intent = hidden.attempts.single()
        assertEquals(actions.first(), intent.action)
        assertTrue(intent.flags and Intent.FLAG_ACTIVITY_NEW_TASK != 0)
    }
    @Test fun missingCastSettingsFallBackToWifiDisplayBeforeWirelessSettings() {
        val fallback = SettingsContext(context).apply { available += actions.drop(1) }
        assertTrue(MiracastSystem.openCastSettings(fallback))
        assertEquals(actions.take(2), fallback.attempts.map { it.action })
    }
    @Test fun wirelessSettingsAreTheFinalFallbackAndNothingAvailableReturnsFalse() {
        val missing = SettingsContext(context)
        assertFalse(MiracastSystem.openCastSettings(missing))
        assertEquals(actions, missing.attempts.map { it.action })
        val fallback = SettingsContext(context).apply { available += actions.last() }
        assertTrue(MiracastSystem.openCastSettings(fallback))
        assertEquals(actions, fallback.attempts.map { it.action })
        assertTrue(fallback.attempts.all { it.flags and Intent.FLAG_ACTIVITY_NEW_TASK != 0 })
    }
    @Test fun inaccessibleOrVanishedActivitiesFallBackInTheSameOrder() {
        val restricted = SettingsContext(context).apply {
            denied += actions.first()
            available += actions.last()
        }
        assertTrue(MiracastSystem.openCastSettings(restricted))
        assertEquals(actions, restricted.attempts.map { it.action })
        restricted.available.clear()
        assertFalse(MiracastSystem.openCastSettings(restricted))
    }
}
