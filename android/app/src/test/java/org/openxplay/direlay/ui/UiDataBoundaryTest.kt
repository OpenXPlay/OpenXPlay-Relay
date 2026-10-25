// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-25
package org.openxplay.direlay.ui

import org.junit.Assert.*
import org.junit.Test
import java.io.File

class UiDataBoundaryTest {
    @Test fun uiSourcesHaveNoRuntimeOrSettingsStore() {
        val path = "src/main/java/org/openxplay/direlay/ui"
        val root = requireNotNull(listOf(File(path), File("app/$path"), File("android/app/$path")).firstOrNull { it.isDirectory })
        val sources = root.walkTopDown().filter { it.isFile && it.extension == "kt" }.toList()
        assertTrue(sources.isNotEmpty())
        val violations = sources.flatMap { source -> source.readLines().mapIndexedNotNull { index, line ->
            if ("RelayRuntime" in line || "RelaySettingsStore" in line) "${source.relativeTo(root)}:${index + 1}" else null
        } }
        assertTrue("UI takes snapshots and callbacks: ${violations.joinToString()}", violations.isEmpty())
    }
}
