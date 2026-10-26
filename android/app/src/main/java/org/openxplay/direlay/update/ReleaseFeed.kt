// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-25
package org.openxplay.direlay.update

import org.json.JSONArray
import org.json.JSONObject
import org.json.JSONTokener
import java.net.URI
import java.time.Instant

enum class UpdateChannel(val preference: String) { STABLE("stable"), BETA("beta") }

data class RelayRelease(val tag: String, val version: ReleaseVersion, val prerelease: Boolean,
    val publishedAt: Instant, val htmlUrl: String, val apkUrl: String?, val body: String)

sealed interface UpdateResult {
    data class Available(val release: RelayRelease) : UpdateResult
    data class UpToDate(val release: RelayRelease) : UpdateResult
    data object NoRelease : UpdateResult
}

fun selectUpdate(releases: List<RelayRelease>, channel: UpdateChannel, installed: String): UpdateResult {
    val candidate = releases.filter { channel == UpdateChannel.BETA || !it.prerelease && it.version.prerelease.isEmpty() }
        .maxByOrNull { it.version } ?: return UpdateResult.NoRelease
    val current = ReleaseVersion.parse(installed)
    return if (current == null || candidate.version > current) UpdateResult.Available(candidate) else UpdateResult.UpToDate(candidate)
}

object ReleaseFeed {
    fun parse(json: String): List<RelayRelease> {
        val tokens = JSONTokener(json)
        val feed = tokens.nextValue() as? JSONArray ?: error("Expected releases")
        check(tokens.nextClean() == '\u0000')
        return buildList {
            for (index in 0 until feed.length()) {
                val item = feed.get(index) as? JSONObject ?: error("Expected release")
                val draft = item.boolean("draft")
                val tag = item.string("tag_name")
                if (draft || !androidTag(tag)) continue
                val version = ReleaseVersion.parse(tag.removePrefix("android-v")) ?: continue
                val prerelease = item.boolean("prerelease")
                val published = Instant.parse(item.string("published_at"))
                val fallback = "https://github.com/OpenXPlay/OpenXPlay-Relay/releases/tag/$tag"
                val page = item.string("html_url").takeIf(::releaseUrl) ?: fallback
                val body = when (val value = item.get("body")) {
                    JSONObject.NULL -> ""
                    is String -> value
                    else -> error("Expected body")
                }
                val assets = item.get("assets") as? JSONArray ?: error("Expected assets")
                val links = buildMap {
                    for (assetIndex in 0 until assets.length()) {
                        val asset = assets.get(assetIndex) as? JSONObject ?: error("Expected asset")
                        put(asset.string("name"), asset.string("browser_download_url"))
                    }
                }
                val apk = links["openxplay-relay-${version.value}.apk"] ?: links["openxplay-relay.apk"]
                val safeApk = apk?.takeIf(::releaseUrl)
                add(RelayRelease(tag, version, prerelease, published, if (apk != null && safeApk == null) fallback else page, safeApk, body))
            }
        }
    }

    private fun androidTag(tag: String): Boolean {
        if (!tag.startsWith("android-v")) return false
        val value = tag.removePrefix("android-v")
        val core = value.substringBefore('-').split('.')
        if (core.size != 3 || core.any { it.length !in 1..3 || it.any { char -> char !in '0'..'9' } || it.length > 1 && it[0] == '0' }) return false
        if ('-' !in value) return true
        val pre = value.substringAfter('-')
        return pre.isNotEmpty() && pre[0] != '-' && pre[0] != '.' && pre.all { ReleaseVersion.identifierChar(it) || it == '.' }
    }
    private fun releaseUrl(value: String): Boolean = runCatching {
        val url = URI(value)
        url.scheme == "https" && url.host == "github.com" && url.rawUserInfo == null && url.port in listOf(-1, 443) &&
            url.rawPath.startsWith("/OpenXPlay/OpenXPlay-Relay/") && url.rawQuery == null && url.rawFragment == null &&
            url.rawPath.split('/').none { it == "." || it == ".." || '%' in it || '\\' in it }
    }.getOrDefault(false)
    private fun JSONObject.string(key: String) = get(key) as? String ?: error("Expected $key")
    private fun JSONObject.boolean(key: String) = get(key) as? Boolean ?: error("Expected $key")
}
