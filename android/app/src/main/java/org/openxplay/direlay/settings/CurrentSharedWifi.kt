// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-15
package org.openxplay.direlay.settings

import android.content.Context
import android.net.wifi.WifiManager

@Suppress("DEPRECATION")
fun currentWifiSsid(context: Context): String? = runCatching {
    context.applicationContext.getSystemService(WifiManager::class.java)?.connectionInfo?.ssid
        ?.trim('"')?.takeUnless { it.isBlank() || it == "<unknown ssid>" }
}.getOrNull()
