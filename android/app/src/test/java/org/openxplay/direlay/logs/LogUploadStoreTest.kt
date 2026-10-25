// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-24
package org.openxplay.direlay.logs

import android.app.Application
import android.content.Context
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config
import java.time.Instant
import java.util.UUID
import java.util.concurrent.Executors

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [33], application = Application::class)
class LogUploadStoreTest {
    private fun preferences() = RuntimeEnvironment.getApplication().getSharedPreferences(UUID.randomUUID().toString(), Context.MODE_PRIVATE)

    @Test fun installIdIsPersistedAcrossInstancesAndConcurrentFirstUploads() {
        val preferences = preferences()
        val executor = Executors.newFixedThreadPool(4)
        try {
            val ids = executor.invokeAll(List(12) { java.util.concurrent.Callable { PreferencesLogUploadStore(preferences).userId() } }).map { it.get() }
            assertEquals(1, ids.toSet().size)
            assertEquals(ids.first(), UUID.fromString(ids.first()).toString())
            assertEquals(ids.first(), PreferencesLogUploadStore(preferences).userId())
            assertNotEquals(ids.first(), PreferencesLogUploadStore(preferences()).userId())
        } finally { executor.shutdownNow() }
    }

    @Test fun receiptRoundTripsWithoutChangingTheUserIdAndSupportsFractionalUtcExpiry() {
        val preferences = preferences()
        val now = Instant.parse("2026-10-24T00:00:00Z").toEpochMilli()
        val store = PreferencesLogUploadStore(preferences, { now })
        val userId = store.userId()
        val id = "0123456789abcdef0123456789abcdef"
        val receipt = UploadResult(id, "$LOG_UPLOAD_BASE_URL/r/$id", "$LOG_UPLOAD_BASE_URL/qr/$id.png",
            "2026-11-23T12:34:56.123456789Z", "a".repeat(64), 42)
        assertNull(store.latest())
        store.save(receipt)
        val reopened = PreferencesLogUploadStore(preferences, { now })
        assertEquals(userId, reopened.userId())
        assertEquals(receipt.toJson().toString(), reopened.latest()?.toJson().toString())
        assertFalse(preferences.getString("latest_receipt", "").orEmpty().contains(userId))
    }

    @Test fun expiredReceiptIsHiddenWithoutDeletingItOrRotatingTheInstallId() {
        val preferences = preferences()
        val expiry = Instant.parse("2026-10-26T12:34:56Z").toEpochMilli()
        var now = expiry - 1
        val store = PreferencesLogUploadStore(preferences, { now })
        val userId = store.userId()
        val id = "0123456789abcdef0123456789abcdef"
        val receipt = UploadResult(id, "$LOG_UPLOAD_BASE_URL/r/$id", "$LOG_UPLOAD_BASE_URL/qr/$id.png",
            Instant.ofEpochMilli(expiry).toString(), "a".repeat(64), 42)
        store.save(receipt)
        val saved = receipt.toJson().toString()
        assertEquals(saved, store.latest()?.toJson().toString())
        assertEquals(userId, store.userId())
        for (at in listOf(expiry, expiry + 1)) {
            now = at
            assertNull(store.latest())
            assertEquals(saved, preferences.getString("latest_receipt", null))
            assertEquals(userId, store.userId())
        }
    }

    @Test fun corruptOrForeignReceiptIsIgnoredWithoutRotatingTheInstallId() {
        val preferences = preferences()
        val store = PreferencesLogUploadStore(preferences)
        val userId = store.userId()
        for (value in listOf("not json", JSONObject().put("tracking_url", "https://example.org").toString())) {
            preferences.edit().putString("latest_receipt", value).commit()
            assertNull(store.latest()); assertEquals(userId, store.userId())
        }
    }
}
