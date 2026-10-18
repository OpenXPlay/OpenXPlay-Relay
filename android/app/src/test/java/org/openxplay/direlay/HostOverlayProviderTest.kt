// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-15
package org.openxplay.direlay

import android.app.Application
import androidx.activity.ComponentActivity
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import org.openxplay.direlay.relay.RelayRuntime
import org.robolectric.Robolectric
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config
import java.util.Locale

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [33], qualifiers = "en", application = Application::class)
class HostOverlayProviderTest {
    @Test fun applicationProviderReturnsNullWhenLocalPreviewIsEnabled() {
        val context = RuntimeEnvironment.getApplication()
        val runtime = RelayRuntime(context)
        try {
            val settings = runtime.settingsStore.awaitSnapshot()
            runtime.settingsStore.save(settings.copy(localPreview = true))
            runtime.settingsStore.awaitSnapshot()
            val application = DiRelayApplication()
            // Avoid starting a second runtime (web servers, discovery) for the Application under test.
            DiRelayApplication::class.java.getDeclaredField("runtime").apply { isAccessible = true }
                .set(application, runtime)
            val activity = Robolectric.buildActivity(ComponentActivity::class.java).get()
            assertNull(application.carPlayHostOverlay(activity))
        } finally { runtime.close() }
    }

    @Test fun overlayCopyIsAvailableInBothLocales() {
        val context = RuntimeEnvironment.getApplication()
        val keys = listOf(R.string.host_connecting, R.string.host_running, R.string.host_keep_running,
            R.string.host_back, R.string.host_cancel, R.string.host_stopped)
        for (tag in listOf("en", "zh-CN")) {
            val configuration = android.content.res.Configuration(context.resources.configuration).apply {
                setLocale(Locale.forLanguageTag(tag))
            }
            val localized = context.createConfigurationContext(configuration)
            keys.forEach { key ->
                assertTrue(localized.getString(key).isNotBlank())
                if (tag != "en") assertNotEquals(context.getString(key), localized.getString(key))
            }
        }
    }
}
