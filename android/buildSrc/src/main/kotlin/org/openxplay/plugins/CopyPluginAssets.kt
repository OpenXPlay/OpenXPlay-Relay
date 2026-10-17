// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-16
package org.openxplay.plugins

import groovy.json.JsonSlurper
import org.gradle.api.DefaultTask
import org.gradle.api.file.DirectoryProperty
import org.gradle.api.tasks.*
import java.io.File
import java.nio.file.Files

/** Optional data input, never a project dependency. NC terms exclude the whole package. */
object PluginAssets {
    fun selected(root: File): List<File> = root.resolve("p").listFiles().orEmpty().filter { dir ->
        runCatching {
            require(dir.name.length <= 64 && Regex("[a-z0-9]+(\\.[a-z0-9-]+)+").matches(dir.name))
            require(!Files.isSymbolicLink(dir.toPath()))
            val manifest = dir.resolve("oxp-plugin.json")
            require(manifest.length() <= 65536 && !Files.isSymbolicLink(manifest.toPath()))
            val value = JsonSlurper().parse(manifest) as Map<*, *>
            val license = value["license"] as String
            value["spec"] == "oxp-web-plugin/0" && value["id"] == dir.name && license.isNotBlank() &&
                license.uppercase().split(Regex("[^A-Z0-9]+" )).none { it == "NC" || it == "NONCOMMERCIAL" }
        }.getOrDefault(false)
    }.sortedBy { it.name }

    fun copy(root: File?, output: File) {
        check(!output.exists() || output.deleteRecursively())
        check(output.mkdirs())
        if (root == null) return
        val destination = output.resolve("plugins")
        fun tree(source: File, target: File) {
            if (Files.isSymbolicLink(source.toPath())) return
            if (source.isDirectory) {
                check(target.isDirectory || target.mkdirs())
                source.listFiles().orEmpty().filter { it.name !in listOf("tests", "tools") && !it.name.startsWith('.') }
                    .forEach { tree(it, target.resolve(it.name)) }
            } else if (source.isFile) {
                target.parentFile.mkdirs()
                source.copyTo(target, overwrite = true)
            }
        }
        if (root.resolve("host").isDirectory) tree(root.resolve("host"), destination.resolve("host"))
        selected(root).forEach { tree(it, destination.resolve("p/${it.name}")) }
    }
}

abstract class CopyPluginAssets : DefaultTask() {
    @get:InputDirectory @get:Optional @get:PathSensitive(PathSensitivity.RELATIVE)
    abstract val pluginDist: DirectoryProperty
    @get:OutputDirectory abstract val outputDirectory: DirectoryProperty
    @TaskAction fun copy() = PluginAssets.copy(pluginDist.orNull?.asFile, outputDirectory.get().asFile)
}
