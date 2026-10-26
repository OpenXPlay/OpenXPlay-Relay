// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-25
package org.openxplay.direlay.update

import android.content.Context
import android.content.SharedPreferences

class UpdatePreferences(private val preferences: SharedPreferences) {
    constructor(context: Context) : this(context.applicationContext.getSharedPreferences("updates", Context.MODE_PRIVATE))
    var channel: UpdateChannel
        get() = if (runCatching { preferences.getString("channel", null) }.getOrNull() == "beta") UpdateChannel.BETA else UpdateChannel.STABLE
        set(value) { preferences.edit().putString("channel", value.preference).apply() }
}
