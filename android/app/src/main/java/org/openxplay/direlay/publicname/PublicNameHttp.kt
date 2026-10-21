// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-20
package org.openxplay.direlay.publicname

import java.io.IOException
import java.io.InputStream
import java.net.HttpURLConnection
import java.net.URL
import java.net.URLEncoder
import java.time.ZonedDateTime
import java.time.format.DateTimeFormatter
import java.util.Locale

/** Problems shown to the user; [key] is the shared error code. */
enum class PublicNameError(val key: String) {
    TOKEN_REJECTED("token-rejected"), NAME_NOT_FOUND("name-not-found"), PROVIDER_RATE_LIMITED("provider-rate-limited"),
    PROVIDER_UNREACHABLE("provider-unreachable"), DNS_NOT_UPDATED("dns-not-updated"), ACME_REJECTED("acme-rejected"),
    ACME_RATE_LIMITED("acme-rate-limited"), NO_ADDRESS("no-address"),
}

/** The message is only the error code: request URLs and provider bodies may carry the token. */
class PublicNameFailure(val error: PublicNameError, val retryAfterMillis: Long? = null, val detail: String? = null) :
    Exception(error.key)

/** URLs may carry the token, so neither class prints them. */
class HttpRequest(val method: String, val url: String, val headers: Map<String, String> = emptyMap(), val body: ByteArray? = null) {
    override fun toString() = "$method request"
}

class HttpResponse(val status: Int, headers: Map<String, List<String>>, val body: ByteArray) {
    private val headers = headers.mapKeys { it.key.lowercase(Locale.ROOT) }
    fun header(name: String): String? = headers[name.lowercase(Locale.ROOT)]?.firstOrNull()
    val text: String get() = String(body, Charsets.UTF_8)
    override fun toString() = "HTTP $status"
}

fun interface PublicNameHttp {
    @Throws(IOException::class) fun send(request: HttpRequest): HttpResponse
}

/**
 * HTTPS with platform trust, 15 s connect and read timeouts, bodies capped at 64 KiB. Redirects are not
 * followed: a request URL or header may carry the token and must reach only the host it was built for.
 */
class UrlConnectionHttp(private val userAgent: String, private val httpsOnly: Boolean = true) : PublicNameHttp {
    override fun send(request: HttpRequest): HttpResponse {
        val url = URL(request.url)
        if (httpsOnly && url.protocol != "https") throw IOException("HTTPS required")
        val connection = url.openConnection() as HttpURLConnection
        try {
            connection.connectTimeout = TIMEOUT_MS
            connection.readTimeout = TIMEOUT_MS
            connection.instanceFollowRedirects = false
            connection.useCaches = false
            connection.requestMethod = request.method
            connection.setRequestProperty("User-Agent", userAgent)
            request.headers.forEach { (key, value) -> connection.setRequestProperty(key, value) }
            request.body?.let { body ->
                connection.doOutput = true
                connection.setFixedLengthStreamingMode(body.size)
                connection.outputStream.use { it.write(body) }
            }
            val status = connection.responseCode
            val stream = if (status >= 400) connection.errorStream else connection.inputStream
            val body = stream?.use { readBounded(it, MAX_BODY_BYTES) } ?: ByteArray(0)
            return HttpResponse(status, connection.headerFields.filterKeys { it != null }, body)
        } finally { connection.disconnect() }
    }

    companion object {
        const val TIMEOUT_MS = 15_000
        const val MAX_BODY_BYTES = 64 * 1024
        fun userAgent(version: String) = "OpenXPlay-Relay/$version (Android)"

        fun readBounded(input: InputStream, limit: Int): ByteArray {
            val buffer = java.io.ByteArrayOutputStream()
            val chunk = ByteArray(8192)
            while (true) {
                val count = input.read(chunk)
                if (count < 0) return buffer.toByteArray()
                if (buffer.size() + count > limit) throw IOException("Response too large")
                buffer.write(chunk, 0, count)
            }
        }
    }
}

internal fun query(vararg parameters: Pair<String, String>): String =
    parameters.joinToString("&") { (key, value) -> "$key=${URLEncoder.encode(value, "UTF-8")}" }

/** Delta-seconds or an HTTP date; null when absent or unreadable. */
internal fun retryAfterMillis(value: String?, now: Long): Long? {
    val text = value?.trim()?.takeIf { it.isNotEmpty() } ?: return null
    text.toLongOrNull()?.let { return if (it < 0) null else it.coerceAtMost(MAX_RETRY_AFTER_S) * 1000 }
    return runCatching { ZonedDateTime.parse(text, DateTimeFormatter.RFC_1123_DATE_TIME).toInstant().toEpochMilli() - now }
        .getOrNull()?.coerceIn(0, MAX_RETRY_AFTER_S * 1000)
}

private const val MAX_RETRY_AFTER_S = 30L * 86_400
