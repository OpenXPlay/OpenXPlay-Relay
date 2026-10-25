// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-16
package org.openxplay.direlay.plugins

import org.openxplay.stream.core.FrameRatePreference
import android.app.Application
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import org.openxplay.direlay.admin.*
import org.openxplay.direlay.settings.RelaySettingsStore
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config
import java.io.File
import java.nio.file.Files

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [28], manifest = Config.NONE, application = Application::class)
class RelayPluginsTest {
    private fun initial() = org.openxplay.direlay.settings.RelaySettings(
        org.openxplay.direlay.settings.ConnectionMode.USB, com.shilapi.xcertplay.orchestration.MfiTarget.I2C,
        "/dev/i2c-0", "", "", null, FrameRatePreference.FPS_60, false, false, false, null)
    private fun packageAt(root: File, id: String, license: String = "MIT") {
        val dir = root.resolve("p/$id").apply { mkdirs() }
        dir.resolve("icon.svg").writeText("<svg/>")
        dir.resolve("oxp-plugin.json").writeText("""{"spec":"oxp-web-plugin/0","id":"$id","name":{"en":"Clock","zh-CN":"时钟"},"version":"1.0.0","license":"$license","icon":"icon.svg"}""")
    }
    @Test fun settingsContractMatchesSharedAdminAndPersistsOrderedIds() {
        val context = RuntimeEnvironment.getApplication()
        context.getSharedPreferences("direlay", 0).edit().clear().commit()
        RelaySettingsStore(context).use { store ->
            val defaults = store.load()
            assertFalse(defaults.plugins)
            assertTrue(defaults.pluginsEnabled.isEmpty())
            val ids = listOf("org.test.second", "org.test.first")
            val result = AdminSettingsMapping.update(defaults, mapOf("plugins" to true, "pluginsEnabled" to ids), false)
            assertTrue(result.errors.toString(), result.errors.isEmpty())
            store.save(result.settings!!)
            assertTrue(store.load().plugins)
            assertEquals(ids, store.load().pluginsEnabled)
            val section = AdminSettingsMapping.schema.single { it.id == "plugins" }.json()
            assertEquals("section.plugins", section.getString("titleKey"))
            assertEquals("plugins", section.getJSONArray("fields").getJSONObject(1).getString("type"))
            assertEquals("hint.plugins", section.getJSONArray("fields").getJSONObject(0).getString("hintKey"))
            val values = AdminSettingsMapping.values(store.load())
            assertEquals(ids, values["pluginsEnabled"])
            for (invalid in listOf(null, true, "org.test.first", listOf(1), listOf("org.test.first", "org.test.first"), listOf("../escape"))) {
                assertEquals(mapOf("pluginsEnabled" to "error.plugins"),
                    AdminSettingsMapping.update(defaults, mapOf("pluginsEnabled" to invalid), false).errors)
            }
        }
    }

    @Test fun catalogIsBoundedContainedAndConfigurationRequiresSwitchAndPresentIds() {
        val root = Files.createTempDirectory("plugins-catalog").toFile()
        val outside = Files.createTempDirectory("plugins-outside").toFile()
        try {
            packageAt(root, "org.test.first")
            packageAt(root, "org.test.second", "MIT AND CC-BY-NC-SA-4.0")
            val plugins = RelayPlugins.installed(root)
            assertEquals(listOf("org.test.first", "org.test.second"), plugins.map { it.id })
            val json = AdminSettings(AdminSettingsMapping.schema, AdminSettingsMapping.values(initial()), plugins).json()
            val catalog = json.getJSONArray("installedPlugins")
            assertFalse(catalog.getJSONObject(0).getBoolean("nonCommercialAssets"))
            assertTrue(catalog.getJSONObject(1).getBoolean("nonCommercialAssets"))
            assertEquals("时钟", catalog.getJSONObject(0).getJSONObject("name").getString("zh-CN"))
            assertTrue(plugins[0].icon!!.startsWith("data:image/svg+xml,%3C"))
            val settings = initial().copy(pluginsEnabled = listOf("org.test.second", "org.test.missing", "org.test.first"))
            assertNull(RelayPlugins.configuration(settings, root, root))
            assertEquals(listOf("org.test.second", "org.test.first"), RelayPlugins.configuration(settings.copy(plugins = true), root, root)!!.enabled)
            assertNull(RelayPlugins.configuration(settings.copy(plugins = true, pluginsEnabled = listOf("org.test.missing")), root, root))
            val manifest = root.resolve("p/org.test.first/oxp-plugin.json")
            val value = JSONObject(manifest.readText()).put("icon", "../../secret.png")
            manifest.writeText(value.toString())
            assertNull(RelayPlugins.installed(root)[0].icon)
            packageAt(outside, "org.test.escape")
            Files.createSymbolicLink(root.resolve("p/org.test.escape").toPath(), outside.resolve("p/org.test.escape").toPath())
            assertEquals(2, RelayPlugins.installed(root).size)
            manifest.writeText("x".repeat(65537))
            assertEquals(1, RelayPlugins.installed(root).size)
        } finally { root.deleteRecursively(); outside.deleteRecursively() }
    }

}
