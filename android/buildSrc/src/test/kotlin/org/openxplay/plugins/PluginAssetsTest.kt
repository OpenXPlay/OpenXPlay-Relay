// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-16
package org.openxplay.plugins

import org.junit.Assert.*
import org.junit.Test
import java.io.File
import java.nio.file.Files

class PluginAssetsTest {
    @Test fun optionalInputAndLicenseSelection() {
        val root = Files.createTempDirectory("plugins-build").toFile()
        try {
            val dist = root.resolve("install")
            fun plugin(id: String, license: String) {
                val packageDir = dist.resolve("p/$id").apply { mkdirs() }
                packageDir.resolve("oxp-plugin.json").writeText("""{"spec":"oxp-web-plugin/0","id":"$id","license":"$license"}""")
                packageDir.resolve("index.html").writeText("plugin")
                packageDir.resolve("tests").mkdir()
                packageDir.resolve("tests/dev.js").writeText("development")
            }
            plugin("org.test.clock", "Apache-2.0 OR MIT")
            plugin("org.test.whale", "(Apache-2.0 OR MIT) AND CC-BY-NC-SA-4.0")
            plugin("org.test.nc", "LicenseRef-NonCommercial")
            dist.resolve("host").mkdir()
            dist.resolve("host/index.html").writeText("host")
            val output = root.resolve("assets")
            PluginAssets.copy(dist, output)
            assertEquals(listOf("org.test.clock"), PluginAssets.selected(dist).map(File::getName))
            assertTrue(output.resolve("plugins/host/index.html").isFile)
            assertTrue(output.resolve("plugins/p/org.test.clock/index.html").isFile)
            assertFalse(output.resolve("plugins/p/org.test.clock/tests").exists())
            assertFalse(output.resolve("plugins/p/org.test.whale").exists())
            PluginAssets.copy(null, output)
            assertTrue(output.listFiles()!!.isEmpty())
        } finally { root.deleteRecursively() }
    }
}
