// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-22
package org.openxplay.direlay.logs

import java.io.File
import java.io.IOException
import java.security.SecureRandom
import javax.crypto.Mac
import javax.crypto.spec.SecretKeySpec

class LogTokens(key: ByteArray) {
    private val key = key.copyOf().also { require(it.size == 32) }
    fun token(value: String, kind: String = "secret"): String {
        if (isPublic(value)) return value
        val mac = Mac.getInstance("HmacSHA256")
        mac.init(SecretKeySpec(key, "HmacSHA256"))
        val digest = mac.doFinal(value.toByteArray(Charsets.UTF_8)).take(3).joinToString("") { "%02x".format(it.toInt() and 255) }
        return "[$kind#$digest${if (kind == "text") " len=${value.length}" else ""}]"
    }
    override fun toString() = "LogTokens([secret])"
    companion object {
        internal val marker = Regex("\\[(?:(?:secret|name|phone|email|text)#[0-9a-f]{6}(?: len=\\d+)?|secret|location|hidden(?: \\d+ bytes)?|redacted(?: \\d+ bytes| location)?)\\]|<(?:binary|data) \\d+ bytes>|<depth limit>")
        private val publicValues = setOf("true", "false", "null", "nil", "none", "empty", "<empty>", "\"\"", "''", "[]", "{}", "…")
        internal fun isPublic(value: String) = value.isEmpty() || value.lowercase() in publicValues || marker.matches(value)
        internal fun outsideMarkers(text: String, replace: (String) -> String): String = buildString {
            var start = 0
            for (match in marker.findAll(text)) {
                append(replace(text.substring(start, match.range.first))); append(match.value)
                start = match.range.last + 1
            }
            append(replace(text.substring(start)))
        }
        fun randomKey() = ByteArray(32).also { SecureRandom().nextBytes(it) }
        @Synchronized fun loadKey(file: File): ByteArray {
            if (file.exists()) return file.readBytes().also { if (it.size != 32) throw IOException("Invalid log token key") }
            check(file.parentFile?.let { it.isDirectory || it.mkdirs() } == true) { "Log token key unavailable" }
            val key = randomKey()
            val pending = File.createTempFile("log-token-", ".key", file.parentFile)
            try {
                pending.outputStream().use { it.write(key); it.fd.sync() }
                if (!pending.renameTo(file)) throw IOException("Could not store log token key")
            } finally { pending.delete() }
            return key
        }
    }
}
