// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-30
package org.openxplay.direlay.logs

/** Pure formatter: a missing, refused or malformed reading affects only its own field. */
class ConnectionContext(private val sources: Map<String, () -> String?> = emptyMap()) {
    fun line(): String = fields.joinToString(" ") { key ->
        val value = runCatching { sources[key]?.invoke() }.getOrNull()
            ?.takeIf { it.length in 1..32 && it.all { c -> c in 'a'..'z' || c in '0'..'9' || c == '_' } }
            ?: "unavailable"
        "$key=$value"
    }
    companion object {
        val fields = listOf("wifi", "hotspot", "btAdapter", "btDiscovering", "btAcl", "a2dp", "hfp",
            "screen", "powerSave", "idle", "standbyBucket", "foreground", "battery", "charging", "thermal")
    }
}
