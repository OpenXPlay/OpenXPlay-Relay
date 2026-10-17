// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-15
package org.openxplay.direlay.admin

import android.content.Context
import org.json.JSONObject

/** MODE_PRIVATE storage is excluded from backup by the application manifest. */
class PrivateAdminAuthStore(context: Context) : AdminAuthStore {
    private val preferences = context.getSharedPreferences("direlay-admin-auth", Context.MODE_PRIVATE)
    @Synchronized override fun read(): AdminAuthState {
        val json = JSONObject(preferences.getString("state", "{}")!!)
        val password = json.optJSONObject("password")?.let { AdminPasswordHash(it.getString("salt"), it.getString("hash")) }
        val sessions = json.optJSONObject("sessions") ?: JSONObject()
        return AdminAuthState(password, sessions.keys().asSequence().associateWith { sessions.getLong(it) })
    }
    @Synchronized override fun write(state: AdminAuthState) {
        val json = obj("sessions" to state.sessions).apply {
            state.password?.let { put("password", obj("salt" to it.salt, "hash" to it.hash)) }
        }
        check(preferences.edit().putString("state", json.toString()).commit()) { "Could not persist admin authentication" }
    }
}
