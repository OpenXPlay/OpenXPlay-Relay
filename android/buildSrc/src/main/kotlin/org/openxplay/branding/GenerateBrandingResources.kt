// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-14
package org.openxplay.branding

import org.gradle.api.DefaultTask
import org.gradle.api.file.ConfigurableFileCollection
import org.gradle.api.file.DirectoryProperty
import org.gradle.api.tasks.*
import org.gradle.work.Incremental
import org.gradle.work.InputChanges

@CacheableTask
abstract class GenerateBrandingResources : DefaultTask() {
    @get:Incremental
    @get:InputFiles
    @get:PathSensitive(PathSensitivity.RELATIVE)
    abstract val libraryResources: ConfigurableFileCollection

    @get:Incremental
    @get:InputFiles
    @get:PathSensitive(PathSensitivity.RELATIVE)
    abstract val manualResources: ConfigurableFileCollection

    @get:OutputDirectory
    abstract val outputDirectory: DirectoryProperty

    @TaskAction
    fun generate(changes: InputChanges) {
        val manual = manualResources.files.flatMap { BrandingResources.keys(it.readText()) }.toSet()
        val locales = sortedMapOf<String, MutableMap<String, String>>()
        libraryResources.files.sortedBy { it.path }.forEach { file ->
            val entries = locales.getOrPut(file.parentFile.name) { sortedMapOf() }
            BrandingResources.overrides(file.readText(), manual).forEach { (key, value) ->
                val previous = entries.put(key, value)
                check(previous == null || previous == value) { "Conflicting branding resource $key in ${file.parentFile.name}" }
            }
        }
        // Recompute the small resource set, but update only changed files on incremental runs.
        val output = outputDirectory.get().asFile
        if (!changes.isIncremental) {
            check(!output.exists() || output.deleteRecursively()) { "Cannot clear $output" }
        }
        val expected = locales.filterValues { it.isNotEmpty() }.map { (locale, entries) ->
            val directory = output.resolve(locale).apply { mkdirs() }
            val target = directory.resolve("direlay_branding.xml")
            val content = BrandingResources.document(entries)
            if (!target.exists() || target.readText() != content) target.writeText(content)
            target
        }.toSet()
        output.walkTopDown().filter { it.isFile && it !in expected }.forEach {
            check(it.delete()) { "Cannot remove stale resource $it" }
        }
    }
}
