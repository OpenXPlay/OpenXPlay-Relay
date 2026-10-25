// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-25
package org.openxplay.direlay.config

import android.app.Application
import android.os.Parcel
import androidx.compose.runtime.saveable.SaverScope
import org.json.JSONArray
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config
import org.openxplay.direlay.settings.*
import org.openxplay.direlay.ui.relaySettingsSaver

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [33], manifest = Config.NONE, application = Application::class)
class CarlinkPictureConfigTest {
    private val context get() = RuntimeEnvironment.getApplication()
    private val custom = (0..6).map { PictureSize(1000 + it * 2, 600) }
    private val meta = ConfigMeta("test", "2026-10-25T00:00:00Z")
    private fun base() = RelaySettingsStore(context).use { it.load().copy(connectionMode = ConnectionMode.USB) }
    private fun decode(settings: JSONObject, current: RelaySettings = base()) = UserConfigCodec.decode(
        ConfigEnvelope.create(UserConfigCodec.FORMAT, meta).put("settings", settings).toString(), current)

    @Test fun settingsRoundTripCustomAndPresetDefaultsWithoutChangingTheDisplayPort() {
        RelaySettingsStore(context).use { store ->
            val initial = base().copy(carlinkPictureSizes = custom.take(5))
            for (default in listOf(null, custom[2], CarlinkPictureSizes.presets[0])) {
                val chosen = initial.copy(carlinkPictureDefault = default)
                store.save(chosen)
                assertEquals(chosen, store.load())
                assertEquals(chosen.displaySettings(), store.current())
                assertEquals(chosen, RelaySettingsStore(context).use { it.load() })
            }
        }
    }
    @Test fun exportImportIncludesSavedSizesAndDefaultButNoTrial() {
        for (default in listOf(null, custom[0], CarlinkPictureSizes.presets.last())) {
            val chosen = base().copy(carlinkPictureSizes = custom.take(5), carlinkPictureDefault = default)
            val text = UserConfigCodec.encode(chosen, DeveloperOverrides(), meta)
            assertFalse(text.contains("carlinkPictureTrial"))
            val imported = UserConfigCodec.decode(text, base())
            assertTrue(imported.errors.toString(), imported.errors.isEmpty())
            assertTrue(imported.ignoredKeys.toString(), imported.ignoredKeys.isEmpty())
            assertEquals(chosen, imported.settings)
        }
    }
    @Test fun importAndPreferenceLoadFilterEntriesAndClearDanglingDefaultsRegardlessOfKeyOrder() {
        val array = JSONArray().put(JSONObject.NULL).put("invalid").put(JSONObject().put("width", 1001).put("height", 600))
            .put(CarlinkPictureSizes.json(CarlinkPictureSizes.presets[0]))
            .put(CarlinkPictureSizes.json(custom[0])).put(CarlinkPictureSizes.json(custom[0]))
        custom.drop(1).forEach { array.put(CarlinkPictureSizes.json(it)) }
        for (default in listOf(custom[0], custom.last(), PictureSize(0, 0), CarlinkPictureSizes.presets[1])) {
            for (defaultFirst in listOf(true, false)) {
                val json = JSONObject()
                if (defaultFirst) json.put("carlinkPictureDefault", CarlinkPictureSizes.json(default))
                json.put("carlinkPictureSizes", array)
                if (!defaultFirst) json.put("carlinkPictureDefault", CarlinkPictureSizes.json(default))
                val imported = decode(json)
                assertTrue(imported.errors.toString(), imported.errors.isEmpty())
                assertEquals(custom.take(5), imported.settings.carlinkPictureSizes)
                val expected = default.takeIf { it in custom.take(5) || it in CarlinkPictureSizes.presets }
                assertEquals(expected, imported.settings.carlinkPictureDefault)
                context.getSharedPreferences("direlay", 0).edit()
                    .putString("carlinkPictureSizes", array.toString())
                    .putString("carlinkPictureDefault", CarlinkPictureSizes.json(default).toString()).apply()
                RelaySettingsStore(context).use { store ->
                    assertEquals(custom.take(5), store.load().carlinkPictureSizes)
                    assertEquals(expected, store.load().carlinkPictureDefault)
                }
            }
        }
        val current = base().copy(carlinkPictureSizes = custom.take(1), carlinkPictureDefault = custom[0])
        assertNull(decode(JSONObject().put("carlinkPictureSizes", JSONArray()), current).settings.carlinkPictureDefault)
        assertEquals(current, decode(JSONObject(), current).settings)
    }
    @Test fun draftSurvivesARealParcelAndLegacyBundlesKeepBaseValues() {
        val initial = base()
        val scope = object : SaverScope { override fun canBeSaved(value: Any) = true }
        val saver = relaySettingsSaver(initial)
        for (default in listOf(null, custom[0], CarlinkPictureSizes.presets[0])) {
            val chosen = initial.copy(carlinkPictureSizes = custom.take(5), carlinkPictureDefault = default)
            val bundle = with(saver) { requireNotNull(scope.save(chosen)) }
            val parcel = Parcel.obtain()
            try {
                parcel.writeBundle(bundle); parcel.setDataPosition(0)
                assertEquals(chosen, saver.restore(requireNotNull(parcel.readBundle(javaClass.classLoader))))
            } finally { parcel.recycle() }
        }
        val legacy = with(saver) { requireNotNull(scope.save(initial)) }.apply {
            remove("carlinkPictureSizes"); remove("carlinkPictureDefault")
        }
        val remembered = initial.copy(carlinkPictureSizes = custom.take(1), carlinkPictureDefault = custom[0])
        assertEquals(remembered, relaySettingsSaver(remembered).restore(legacy))
        val automatic = with(saver) { requireNotNull(scope.save(initial)) }
        assertEquals(initial, relaySettingsSaver(remembered).restore(automatic))
        assertTrue(automatic.containsKey("carlinkPictureDefault"))
    }
}
