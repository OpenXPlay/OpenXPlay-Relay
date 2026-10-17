// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-16
package org.openxplay.direlay.plugins

import org.junit.Assert.*
import org.junit.Test
import java.nio.file.Files

class BundledPluginsTest {
    @Test fun bundlesRefreshOnApkUpdateAndKeepStorageOutsidePackageRoot() {
        val files = Files.createTempDirectory("plugins-install").toFile()
        var version = "1"
        val contents = linkedMapOf("plugins/host/index.html" to "v1", "plugins/p/org.test.clock/index.html" to "clock")
        var reads = 0
        val bundle = object : PluginBundle {
            override fun list(path: String) = contents.keys.filter { it.startsWith("$path/") }
                .map { it.removePrefix("$path/").substringBefore('/') }.distinct()
            override fun open(path: String) = contents.getValue(path).byteInputStream().also { reads++ }
        }
        try {
            val installer = BundledPlugins(files, { version }, bundle)
            val root = installer.root()
            assertEquals("v1", root.resolve("host/index.html").readText())
            installer.root()
            assertEquals(2, reads)
            files.resolve("plugins/storage").mkdirs()
            files.resolve("plugins/storage/org.test.clock.json").writeText("{}")
            version = "2"
            contents.clear()
            contents["plugins/host/index.html"] = "v2"
            installer.root()
            assertEquals("v2", root.resolve("host/index.html").readText())
            assertFalse(root.resolve("p/org.test.clock").exists())
            assertTrue(files.resolve("plugins/storage/org.test.clock.json").isFile)
            version = "3"; contents.clear(); installer.root()
            assertTrue(RelayPlugins.installed(root).isEmpty())
        } finally { files.deleteRecursively() }
    }
}
