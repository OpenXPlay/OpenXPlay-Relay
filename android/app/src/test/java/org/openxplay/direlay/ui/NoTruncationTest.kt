// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-24
package org.openxplay.direlay.ui

import org.junit.Assert.*
import org.junit.Test
import java.io.File

class NoTruncationTest {
    @Test fun uiSourcesDoNotUseEllipsisOverflow() {
        val path = "src/main/java/org/openxplay/direlay/ui"
        val root = requireNotNull(listOf(File(path), File("app/$path"), File("android/app/$path")).firstOrNull { it.isDirectory }) {
            "Cannot find UI sources from ${System.getProperty("user.dir")}"
        }
        val sources = root.walkTopDown().filter { it.isFile && it.extension == "kt" }.toList()
        assertTrue("No Kotlin UI sources found", sources.isNotEmpty())
        val violations = sources.flatMap { source -> source.readLines().mapIndexedNotNull { index, line ->
            if ("TextOverflow.Ellipsis" in line) "${source.relativeTo(root)}:${index + 1}" else null
        } }
        assertTrue("UI text must wrap: ${violations.joinToString()}", violations.isEmpty())
    }
}
