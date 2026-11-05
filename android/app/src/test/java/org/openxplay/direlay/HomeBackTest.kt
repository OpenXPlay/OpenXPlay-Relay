// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-11-04
package org.openxplay.direlay

import android.app.Application
import android.content.Intent
import androidx.activity.ComponentActivity
import com.shilapi.xcertplay.CarPlayHostActivity
import com.shilapi.xcertplay.SameTaskCarPlayHostActivity
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.Robolectric
import org.robolectric.RobolectricTestRunner
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [33], application = Application::class)
class HomeBackTest {
    @Test fun eitherCarPlayHostBehindHomeIsTheRunningPage() {
        assertTrue(HomeBack.runningPageBehind(listOf(RelayActivity::class.java.name, CarPlayHostActivity::class.java.name)))
        assertTrue(HomeBack.runningPageBehind(listOf(null, SameTaskCarPlayHostActivity::class.java.name)))
        assertFalse(HomeBack.runningPageBehind(listOf(RelayActivity::class.java.name, null)))
        assertFalse(HomeBack.runningPageBehind(emptyList()))
    }

    @Test fun withoutTheRunningPageHomeKeepsTheSystemBack() {
        val activity = Robolectric.buildActivity(ComponentActivity::class.java).setup().get()
        val back = HomeBack(activity).also { activity.onBackPressedDispatcher.addCallback(activity, it) }
        back.refresh()
        assertFalse(back.isEnabled)
    }

    @Test fun backWithTheRunningPageBehindOpensTheLauncher() {
        val activity = Robolectric.buildActivity(ComponentActivity::class.java).setup().get()
        val back = HomeBack(activity).also { activity.onBackPressedDispatcher.addCallback(activity, it) }
        back.isEnabled = true
        activity.onBackPressedDispatcher.onBackPressed()
        val started = shadowOf(activity).nextStartedActivity
        assertEquals(Intent.ACTION_MAIN, started.action)
        assertTrue(started.hasCategory(Intent.CATEGORY_HOME))
        assertFalse(activity.isFinishing)
    }
}
