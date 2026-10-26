// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-24
package org.openxplay.direlay.logs

import android.content.SharedPreferences
import org.json.JSONObject
import java.time.Instant
import java.util.UUID

interface LogUploadStore {
    fun userId(): String
    fun latest(): UploadResult?
    fun save(receipt: UploadResult)
}

/** A random install ID, independent of pairing, authentication keys, and hardware identifiers. */
class PreferencesLogUploadStore(private val preferences: SharedPreferences,
    private val now: () -> Long = System::currentTimeMillis) : LogUploadStore {
    override fun userId(): String = synchronized(identityLock) {
        preferences.getString("user_id", null)?.takeIf(::validLogUserId) ?: UUID.randomUUID().toString().also {
            check(preferences.edit().putString("user_id", it).commit()) { "Unable to persist log user ID" }
        }
    }

    override fun latest(): UploadResult? = runCatching {
        preferences.getString("latest_receipt", null)?.let { UploadResult.fromJson(JSONObject(it)) }
            ?.takeIf { Instant.parse(it.expiresAt).isAfter(Instant.ofEpochMilli(now())) }
    }.getOrNull()

    override fun save(receipt: UploadResult) {
        preferences.edit().putString("latest_receipt", receipt.toJson().toString()).apply()
    }

    private companion object { val identityLock = Any() }
}
