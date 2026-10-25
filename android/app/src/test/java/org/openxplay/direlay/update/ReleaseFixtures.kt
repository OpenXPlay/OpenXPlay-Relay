// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-25
package org.openxplay.direlay.update

import org.json.JSONArray
import org.json.JSONObject

internal object ReleaseFixtures {
    const val REPO = "https://github.com/OpenXPlay/OpenXPlay-Relay"
    val body = """
        Android app: faster pairing · 安卓应用：更快配对

        Pairing improvements for **Carlink**.

        **New**
        - A `beta` option
        - A second option

        **Notes**
        - [Guide](https://openxplay.org/en/)

        **新增**
        - `测试版`选项
        - 第二项

        **说明**
        - [指南](https://openxplay.org/)
        ---
        Commit: private-build-provenance
        Assets: SHA256SUMS
    """.trimIndent()

    fun asset(name: String, url: String) = JSONObject().put("name", name).put("browser_download_url", url)
    fun release(tag: String = "android-v0.1.5", prerelease: Boolean = false, draft: Boolean = false): JSONObject {
        val version = tag.removePrefix("android-v")
        return JSONObject().put("tag_name", tag).put("draft", draft).put("prerelease", prerelease)
            .put("published_at", "2026-10-25T12:00:00Z").put("html_url", "$REPO/releases/tag/$tag")
            .put("body", body).put("assets", JSONArray()
                .put(asset("openxplay-relay.apk", "$REPO/releases/download/$tag/openxplay-relay.apk"))
                .put(asset("SHA256SUMS", "$REPO/releases/download/$tag/SHA256SUMS"))
                .put(asset("openxplay-relay-$version.apk", "$REPO/releases/download/$tag/openxplay-relay-$version.apk")))
            .put("unknown_future_field", JSONObject().put("anything", true))
    }
    fun feed() = JSONArray().put(release("android-v0.1.5-beta.2", true)).put(release("linux-v0.1.6-beta.1", true))
        .put(release("v0.1.3")).put(release("android-v0.1.7", draft = true))
        .put(release("android-v0.1.5-beta.3")).put(release("android-v0.1.6-beta.1", true)
            .put("published_at", "2026-10-01T12:00:00Z"))
        .put(release()).toString()
}
