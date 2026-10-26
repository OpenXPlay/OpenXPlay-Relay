// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-23
package org.openxplay.direlay.logs

import org.json.JSONObject

/** Decode RTSP text before filtering, so escaped newlines cannot hide credential keys. */
object TracePrivacy {
    fun redact(line: String, redactor: LogRedactor): String {
        val json = runCatching { JSONObject(line) }.getOrNull() ?: return redactor.redact(line)
        if (json.optString("type") == "Rtsp") {
            val body = json.optString("body")
            val stream = LogRedactorStream(redactor)
            json.put("body", body.splitToSequence('\n').joinToString("\n") { stream.redact(it) })
            val headers = json.optJSONObject("headers")
            headers?.keys()?.asSequence()?.toList()?.forEach { key ->
                headers.put(key, redactor.redact("$key: ${headers.get(key)}").substringAfter(": "))
            }
        }
        return redactor.redact(json.toString())
    }
}
