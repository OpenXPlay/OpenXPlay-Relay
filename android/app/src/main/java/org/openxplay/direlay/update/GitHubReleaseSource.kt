// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-25
package org.openxplay.direlay.update

import kotlinx.coroutines.*
import java.io.ByteArrayOutputStream
import java.io.IOException
import java.net.HttpURLConnection
import java.net.SocketTimeoutException
import java.net.URL
import java.time.Instant
import java.util.concurrent.atomic.AtomicReference
import kotlin.coroutines.resume
import kotlin.coroutines.resumeWithException

sealed class UpdateFailure : Exception() {
    data object NoInternet : UpdateFailure()
    data object Unreachable : UpdateFailure()
    data object Timeout : UpdateFailure()
    data class RateLimited(val resetAt: Instant?) : UpdateFailure()
    data class Http(val status: Int) : UpdateFailure()
    data object InvalidResponse : UpdateFailure()
}
fun interface ReleaseSource { suspend fun fetch(): List<RelayRelease> }
fun interface ReleaseConnectionFactory { fun open(url: URL): HttpURLConnection }

class GitHubReleaseSource(private val connections: ReleaseConnectionFactory, private val version: String,
    private val timeoutMillis: Long = 40_000) : ReleaseSource {
    private val workers = CoroutineScope(SupervisorJob() + Dispatchers.IO)

    override suspend fun fetch(): List<RelayRelease> = withTimeoutOrNull(timeoutMillis) {
        suspendCancellableCoroutine { continuation ->
            val active = AtomicReference<HttpURLConnection?>()
            val worker = workers.launch {
                try {
                    var url = URL(ENDPOINT)
                    var redirects = 0
                    var releases: List<RelayRelease>? = null
                    while (releases == null) {
                        if (!continuation.isActive) throw CancellationException()
                        val connection = connections.open(url)
                        active.set(connection)
                        try {
                            if (!continuation.isActive) throw CancellationException()
                            connection.apply {
                                requestMethod = "GET"
                                connectTimeout = 15_000; readTimeout = 20_000
                                instanceFollowRedirects = false; useCaches = false
                                setRequestProperty("Accept", "application/vnd.github+json")
                                setRequestProperty("X-GitHub-Api-Version", "2022-11-28")
                                setRequestProperty("User-Agent", "OpenXPlay-Relay-Android/$version")
                            }
                            val status = connection.responseCode
                            if (status in listOf(301, 302, 303, 307, 308)) {
                                val target = connection.getHeaderField("Location")?.let { runCatching { URL(url, it) }.getOrNull() }
                                if (target == null || !apiUrl(target) || redirects++ >= 5) throw UpdateFailure.Http(status)
                                url = target
                                continue
                            }
                            if (status != 200) {
                                if (status in listOf(403, 429) && connection.getHeaderField("x-ratelimit-remaining") == "0") {
                                    val reset = connection.getHeaderField("x-ratelimit-reset")?.toLongOrNull()?.let {
                                        runCatching { Instant.ofEpochSecond(it) }.getOrNull()
                                    }
                                    throw UpdateFailure.RateLimited(reset)
                                }
                                throw UpdateFailure.Http(status)
                            }
                            if (connection.contentLengthLong > MAX_BODY_BYTES) throw UpdateFailure.InvalidResponse
                            val body = connection.inputStream.use { input ->
                                val output = ByteArrayOutputStream()
                                val buffer = ByteArray(8192)
                                while (true) {
                                    if (!continuation.isActive) throw CancellationException()
                                    val size = input.read(buffer, 0, minOf(buffer.size, MAX_BODY_BYTES + 1 - output.size()))
                                    if (size < 0) break
                                    output.write(buffer, 0, size)
                                    if (output.size() > MAX_BODY_BYTES) throw UpdateFailure.InvalidResponse
                                }
                                output.toString(Charsets.UTF_8.name())
                            }
                            releases = ReleaseFeed.parse(body)
                        } finally { active.compareAndSet(connection, null); runCatching { connection.disconnect() } }
                    }
                    if (continuation.isActive) continuation.resume(releases)
                } catch (failure: Exception) {
                    if (continuation.isActive) continuation.resumeWithException(when (failure) {
                        is UpdateFailure -> failure
                        is SocketTimeoutException -> UpdateFailure.Timeout
                        is IOException -> UpdateFailure.Unreachable
                        else -> UpdateFailure.InvalidResponse
                    })
                }
            }
            continuation.invokeOnCancellation { active.getAndSet(null)?.let { runCatching { it.disconnect() } }; worker.cancel() }
        }
    } ?: throw UpdateFailure.Timeout

    private fun apiUrl(url: URL) = url.protocol == "https" && url.host == "api.github.com" &&
        url.userInfo == null && url.port in listOf(-1, 443) && url.ref == null
    companion object {
        const val ENDPOINT = "https://api.github.com/repos/OpenXPlay/OpenXPlay-Relay/releases?per_page=30"
        const val MAX_BODY_BYTES = 2 * 1024 * 1024
    }
}
