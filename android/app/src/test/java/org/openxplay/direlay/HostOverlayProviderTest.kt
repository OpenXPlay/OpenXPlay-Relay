// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-15
package org.openxplay.direlay

import android.app.Application
import android.widget.FrameLayout
import android.widget.ImageButton
import androidx.activity.ComponentActivity
import androidx.compose.ui.platform.ComposeView
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import org.openxplay.direlay.relay.RelayRuntime
import org.openxplay.direlay.presentation.createCarPlayHostOverlay
import org.openxplay.direlay.presentation.HostOverlayProducer
import org.openxplay.direlay.settings.*
import org.robolectric.Robolectric
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config
import java.util.Locale

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [33], qualifiers = "en", application = Application::class)
class HostOverlayProviderTest {
    @Test fun applicationProviderReturnsOnlyABackButtonWhenLocalPreviewIsEnabled() {
        val context = RuntimeEnvironment.getApplication()
        val runtime = RelayRuntime(context)
        try {
            runtime.settingsStore.awaitSnapshot()
            runtime.settingsStore.saveDeveloperOverrides(runtime.settingsStore.developerOverrides.value.copy(active = true, localPreview = true))
            runtime.settingsStore.awaitSnapshot()
            val application = DiRelayApplication()
            // Avoid starting a second runtime (web servers, discovery) for the Application under test.
            DiRelayApplication::class.java.getDeclaredField("runtime").apply { isAccessible = true }
                .set(application, runtime)
            val activity = Robolectric.buildActivity(ComponentActivity::class.java).get()
            val overlay = application.carPlayHostOverlay(activity) as FrameLayout
            assertFalse(overlay.isClickable)
            assertEquals(1, overlay.childCount)
            assertTrue(overlay.getChildAt(0) is ImageButton)
            assertEquals(activity.getString(R.string.host_back), overlay.getChildAt(0).contentDescription)
        } finally { runtime.close() }
    }

    @Test fun allOutputSetsUseEffectivePreviewBeforeRuntimeReloadWithoutSavingIt() {
        val context = RuntimeEnvironment.getApplication()
        val runtime = RelayRuntime(context)
        try {
            val store = runtime.settingsStore
            val base = store.awaitSnapshot()
            val activity = Robolectric.buildActivity(ComponentActivity::class.java).get()
            val producer = HostOverlayProducer(context, runtime)
            assertEquals(5, RelayOutput.entries.size)
            for (mask in 1 until (1 shl RelayOutput.entries.size)) for (active in listOf(false, true)) for (preview in listOf(false, true)) {
                val outputs = RelayOutput.entries.filterIndexed { index, _ -> mask and (1 shl index) != 0 }.toSet()
                val developer = DeveloperOverrides(active = active, localPreview = preview)
                store.save(base.copy(outputs = outputs, carBluetoothAudio = false), developer)
                val settings = store.awaitSnapshot()
                val effectivePreview = active && preview || RelayOutput.BROWSER !in outputs && RelayOutput.CARLINK !in outputs && RelayOutput.CARLIFE !in outputs
                val overlay = createCarPlayHostOverlay(activity, runtime)
                assertEquals("$outputs $developer", effectivePreview, overlay is FrameLayout)
                assertEquals("$outputs $developer", !effectivePreview, overlay is ComposeView)
                (overlay as? ComposeView)?.disposeComposition()
                assertEquals(effectivePreview, producer.sample(0, false)?.localPreview)
                assertEquals(active && preview, settings.localPreview)
                assertFalse(settings.playCarPlayAudio)
                assertEquals(MicrophoneSource.HEAD_UNIT, settings.microphoneSource)
                assertEquals(settings, store.load())
                assertEquals(developer, store.developerOverrides.value)
            }
        } finally { runtime.close() }
    }

    @Test fun overlayCopyIsAvailableInBothLocales() {
        val context = RuntimeEnvironment.getApplication()
        val keys = listOf(R.string.host_connecting, R.string.host_running, R.string.host_keep_running,
            R.string.host_back, R.string.host_cancel, R.string.host_stopped, R.string.stage_show_carplay,
            R.string.setup_wireless_display_hint, R.string.option_wireless_display_hint,
            R.string.audio_wireless_forwarding)
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
