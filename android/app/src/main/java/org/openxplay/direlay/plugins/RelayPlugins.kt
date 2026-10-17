// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-16
package org.openxplay.direlay.plugins

import android.content.Context
import org.json.JSONObject
import org.openxplay.direlay.admin.AdminJson
import org.openxplay.direlay.settings.RelaySettings
import org.openxplay.stream.web.PluginsConfig
import java.io.File

data class InstalledPlugin(val id: String, val name: Map<String, String>, val version: String,
    val license: String, val icon: String?, val nonCommercialAssets: Boolean) : AdminJson {
    override fun json() = JSONObject().put("id", id).put("name", JSONObject(name)).put("version", version)
        .put("license", license).put("icon", icon ?: JSONObject.NULL).put("nonCommercialAssets", nonCommercialAssets)
}

object RelayPlugins {
    fun validId(id: String) = id.length <= 64 && Regex("[a-z0-9]+(\\.[a-z0-9-]+)+").matches(id)
    fun validIds(value: Any?): Boolean = value is List<*> && value.all { it is String && validId(it) } && value.distinct().size == value.size
    private fun read(root: File, relative: String): ByteArray? = runCatching {
        require(relative.split('/').all { it.isNotEmpty() && !it.startsWith('.') && '\\' !in it && ':' !in it })
        val file = root.resolve(relative).canonicalFile
        require(file.toPath().startsWith(root.canonicalFile.toPath()) && file.isFile && file.length() <= 65536)
        file.inputStream().use { input ->
            val output = java.io.ByteArrayOutputStream()
            val buffer = ByteArray(4096)
            while (true) {
                val count = input.read(buffer)
                if (count < 0) break
                require(output.size() + count <= 65536)
                output.write(buffer, 0, count)
            }
            output.toByteArray()
        }
    }.getOrNull()

    fun installed(root: File): List<InstalledPlugin> = root.resolve("p").listFiles().orEmpty().mapNotNull { dir ->
        runCatching {
            val id = dir.name
            require(validId(id))
            val manifest = JSONObject(String(requireNotNull(read(root, "p/$id/oxp-plugin.json")), Charsets.UTF_8))
            require(manifest.getString("id") == id && manifest.getString("spec") == "oxp-web-plugin/0")
            val names = manifest.getJSONObject("name")
            val name = names.keys().asSequence().associateWith { names.getString(it) }
            require(name.containsKey("en"))
            val license = manifest.getString("license")
            val iconPath = manifest.optString("icon")
            val mime = when (iconPath.substringAfterLast('.')) { "svg" -> "image/svg+xml"; "png" -> "image/png"; else -> null }
            val icon = mime?.let { type -> read(dir, iconPath)?.let { bytes ->
                "data:$type," + bytes.joinToString("") { "%%%02X".format(it.toInt() and 255) }
            } }
            InstalledPlugin(id, name, manifest.getString("version"), license, icon,
                license.uppercase().split(Regex("[^A-Z0-9]+" )).any { it == "NC" || it == "NONCOMMERCIAL" })
        }.getOrNull()
    }.sortedBy { it.id }

    fun configuration(settings: RelaySettings, root: File, state: File): PluginsConfig? {
        if (!settings.plugins) return null
        val present = installed(root).map { it.id }.toSet()
        val enabled = settings.pluginsEnabled.filter { it in present }
        return enabled.takeIf { it.isNotEmpty() }?.let { PluginsConfig(root.toPath(), it, state.toPath(), ANDROID_TOPICS) }
    }

    /** The relay topics the Android relay produces; now playing and call are not provided here. */
    val ANDROID_TOPICS = listOf("session", "activity", "route", "audioSource")
}

/** Replaces bundled data after APK updates; persistent plugin storage lives outside this root. */
internal interface PluginBundle {
    fun list(path: String): List<String>
    fun open(path: String): java.io.InputStream
}
class BundledPlugins internal constructor(private val files: File, private val version: () -> String, private val assets: PluginBundle) {
    constructor(context: Context) : this(context.filesDir, {
        context.packageManager.getPackageInfo(context.packageName, 0).let { "${it.versionCode}:${it.lastUpdateTime}" }
    }, object : PluginBundle {
        override fun list(path: String) = context.assets.list(path).orEmpty().toList()
        override fun open(path: String) = context.assets.open(path)
    })
    @Synchronized fun root(): File {
        val root = File(files, "plugin-packages")
        val version = version()
        val marker = File(root, "bundle-version")
        if (marker.isFile && marker.readText() == version) return root
        val staging = File(files, "plugin-packages-staging")
        check(!staging.exists() || staging.deleteRecursively())
        check(staging.mkdirs())
        fun copy(asset: String, target: File) {
            val children = assets.list(asset)
            if (children.isNotEmpty()) {
                check(target.isDirectory || target.mkdirs())
                children.forEach { copy("$asset/$it", File(target, it)) }
            } else assets.open(asset).use { input -> target.outputStream().use { input.copyTo(it) } }
        }
        assets.list("plugins").forEach { copy("plugins/$it", File(staging, it)) }
        File(staging, "bundle-version").writeText(version)
        check(!root.exists() || root.deleteRecursively())
        check(staging.renameTo(root))
        return root
    }
}
