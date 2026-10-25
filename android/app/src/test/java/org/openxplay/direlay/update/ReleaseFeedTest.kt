// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-25
package org.openxplay.direlay.update

import android.app.Application
import org.json.JSONArray
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [33], application = Application::class)
class ReleaseFeedTest {
    private fun parse(release: JSONObject) = ReleaseFeed.parse(JSONArray().put(release).toString()).single()
    private fun available(result: UpdateResult) = (result as UpdateResult.Available).release.version.value

    @Test fun highestSemverWinsInEachChannelRegardlessOfDateAndOtherPlatforms() {
        val releases = ReleaseFeed.parse(ReleaseFixtures.feed())
        assertEquals(4, releases.size)
        assertEquals("0.1.5", available(selectUpdate(releases, UpdateChannel.STABLE, "0.1.2")))
        assertEquals("0.1.6-beta.1", available(selectUpdate(releases, UpdateChannel.BETA, "0.1.2")))
        for (installed in listOf("0.1.5", "0.1.6-beta.1")) {
            val result = selectUpdate(releases, UpdateChannel.STABLE, installed) as UpdateResult.UpToDate
            assertEquals("0.1.5", result.release.version.value)
        }
        assertEquals("0.1.5", available(selectUpdate(releases, UpdateChannel.STABLE, "unparseable")))
        assertEquals("0.1.5", available(selectUpdate(releases, UpdateChannel.STABLE, "0.0.0-dev.abc1234")))
        assertEquals(UpdateResult.NoRelease, selectUpdate(emptyList(), UpdateChannel.BETA, "0.1.2"))
        assertEquals(UpdateResult.NoRelease, selectUpdate(releases.filter { it.version.prerelease.isNotEmpty() }, UpdateChannel.STABLE, "0.1.2"))
    }
    @Test fun betaUsersAlsoReceiveANewerFinal() {
        val releases = ReleaseFeed.parse(JSONArray().put(ReleaseFixtures.release("android-v0.1.6"))
            .put(ReleaseFixtures.release("android-v0.1.6-beta.10", true)).toString())
        assertEquals("0.1.6", available(selectUpdate(releases, UpdateChannel.BETA, "0.1.6-beta.10")))
    }
    @Test fun tagLimitsAndAndroidPrefixAreEnforced() {
        for (tag in listOf("v0.1.5", "linux-v0.1.5", "android-v01.1.5", "android-v1000.1.5", "android-v0.1.5+build", "android-v0.1.5-", "android-v0.1.5-{beta}")) {
            assertTrue(tag, ReleaseFeed.parse(JSONArray().put(ReleaseFixtures.release(tag)).toString()).isEmpty())
        }
    }
    @Test fun versionedApkWinsThenFixedApkThenTheReleasePage() {
        val release = parse(ReleaseFixtures.release())
        assertEquals("${ReleaseFixtures.REPO}/releases/download/android-v0.1.5/openxplay-relay-0.1.5.apk", release.apkUrl)
        val fixed = ReleaseFixtures.release().put("assets", JSONArray().put(ReleaseFixtures.asset("openxplay-relay.apk", "${ReleaseFixtures.REPO}/releases/download/android-v0.1.5/openxplay-relay.apk")))
        assertEquals("${ReleaseFixtures.REPO}/releases/download/android-v0.1.5/openxplay-relay.apk", parse(fixed).apkUrl)
        val none = parse(ReleaseFixtures.release().put("assets", JSONArray()))
        assertNull(none.apkUrl); assertEquals("${ReleaseFixtures.REPO}/releases/tag/android-v0.1.5", none.htmlUrl)
    }
    @Test fun unsafeUrlsFallBackToTheTagPageWithoutOpeningAnotherOriginOrRepository() {
        val fallback = "${ReleaseFixtures.REPO}/releases/tag/android-v0.1.5"
        for (url in listOf("https://evil.example/file.apk", "http://github.com/OpenXPlay/OpenXPlay-Relay/file.apk",
            "https://github.com.evil.example/OpenXPlay/OpenXPlay-Relay/file.apk", "https://user@github.com/OpenXPlay/OpenXPlay-Relay/file.apk",
            "https://github.com/OpenXPlay/Other/file.apk", "https://github.com/OpenXPlay/OpenXPlay-Relay/../Other/file.apk",
            "https://github.com/OpenXPlay/OpenXPlay-Relay/%2e%2e/Other/file.apk", "https://github.com:444/OpenXPlay/OpenXPlay-Relay/file.apk")) {
            val release = parse(ReleaseFixtures.release().put("html_url", url).put("assets", JSONArray().put(ReleaseFixtures.asset("openxplay-relay.apk", url))))
            assertNull(url, release.apkUrl); assertEquals(url, fallback, release.htmlUrl)
        }
    }
    @Test fun wrongTypesMalformedJsonAndInvalidDatesAreRejectedWithoutCoercion() {
        val invalid = listOf("not json", "{}", "[] trailing", "[1]",
            JSONArray().put(ReleaseFixtures.release().put("draft", "false")).toString(),
            JSONArray().put(ReleaseFixtures.release().put("prerelease", 0)).toString(),
            JSONArray().put(ReleaseFixtures.release().put("body", 123)).toString(),
            JSONArray().put(ReleaseFixtures.release().put("published_at", "not a date")).toString(),
            JSONArray().put(ReleaseFixtures.release().put("html_url", JSONObject.NULL)).toString(),
            JSONArray().put(ReleaseFixtures.release().put("assets", JSONObject())).toString(),
            JSONArray().put(ReleaseFixtures.release().put("assets", JSONArray().put(JSONObject().put("name", 1).put("browser_download_url", "https://github.com/")))).toString())
        invalid.forEach { json -> assertThrows(Exception::class.java) { ReleaseFeed.parse(json) } }
        assertEquals("", parse(ReleaseFixtures.release().put("body", JSONObject.NULL)).body)
    }
}
