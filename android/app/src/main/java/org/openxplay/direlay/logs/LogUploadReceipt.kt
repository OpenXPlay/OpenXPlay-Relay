// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-24
package org.openxplay.direlay.logs

import org.json.JSONObject
import java.time.Instant

const val LOG_UPLOAD_BASE_URL = "https://logs.openxplay.org"
const val LOG_UPLOAD_MAX_FILE_BYTES = 52_428_800L

internal fun validLogUserId(value: String) = Regex("^[A-Za-z0-9][A-Za-z0-9._:-]{0,127}$").matches(value)

class UploadResult(val id: String, val link: String, val qrCodeUrl: String, val expiresAt: String,
    val sha256: String, val bytes: Long) {
    override fun toString() = "UploadResult([redacted])"

    internal fun toJson() = JSONObject().put("id", id).put("tracking_url", link).put("qr_code_url", qrCodeUrl)
        .put("expires_at", expiresAt).put("sha256", sha256).put("bytes", bytes)

    companion object {
        internal fun fromJson(json: JSONObject, publicUrl: String = LOG_UPLOAD_BASE_URL): UploadResult {
            val id = json.getString("id")
            val link = json.getString("tracking_url")
            val qr = json.getString("qr_code_url")
            val expires = json.getString("expires_at")
            val hash = json.getString("sha256")
            val bytes = json.getLong("bytes")
            require(Regex("^[a-f0-9]{32}$").matches(id))
            require(link == "$publicUrl/r/$id" && qr == "$publicUrl/qr/$id.png")
            Instant.parse(expires).toEpochMilli()
            require(Regex("^[a-f0-9]{64}$").matches(hash) && bytes in 1L..LOG_UPLOAD_MAX_FILE_BYTES)
            return UploadResult(id, link, qr, expires, hash, bytes)
        }
    }
}
