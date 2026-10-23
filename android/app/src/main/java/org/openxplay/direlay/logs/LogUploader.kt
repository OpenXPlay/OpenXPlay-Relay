// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-22
package org.openxplay.direlay.logs

import android.net.ConnectivityManager
import android.net.NetworkCapabilities
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.launch
import kotlinx.coroutines.suspendCancellableCoroutine
import org.json.JSONObject
import org.openxplay.direlay.publicname.UrlConnectionHttp
import java.io.File
import java.net.HttpURLConnection
import java.net.URL
import java.util.UUID
import java.util.concurrent.atomic.AtomicReference
import kotlin.coroutines.resume
import kotlin.coroutines.resumeWithException

interface LogUploader { suspend fun upload(file: File, name: String): UploadResult }
class UploadResult(val link: String) { override fun toString() = "UploadResult([redacted])" }
class LogUploadFailure(val reason: String, val status: Int? = null) : Exception(if (status == null) reason else "$reason (HTTP $status)")
fun interface UploadConnectionFactory { fun open(url: URL): HttpURLConnection }

class InternetUploadConnection(private val connectivity: ConnectivityManager) : UploadConnectionFactory {
    override fun open(url: URL): HttpURLConnection {
        fun validated(network: android.net.Network): Boolean = connectivity.getNetworkCapabilities(network)?.let {
            it.hasCapability(NetworkCapabilities.NET_CAPABILITY_INTERNET) && it.hasCapability(NetworkCapabilities.NET_CAPABILITY_VALIDATED)
        } == true
        val network = connectivity.activeNetwork?.takeIf(::validated) ?: connectivity.allNetworks.firstOrNull(::validated)
            ?: throw LogUploadFailure("no-internet")
        return network.openConnection(url) as HttpURLConnection
    }
}

class TmpfileLinkUploader(private val connections: UploadConnectionFactory, private val version: String,
    private val baseUrl: String = "https://tmpfile.link") : LogUploader {
    override suspend fun upload(file: File, name: String): UploadResult = coroutineScope {
        if (file.length() > MAX_FILE_BYTES) throw LogUploadFailure("too-large")
        val url = URL(baseUrl.trimEnd('/') + "/api/upload")
        if (url.protocol != "https") throw LogUploadFailure("https-required")
        suspendCancellableCoroutine { continuation ->
            val active = AtomicReference<HttpURLConnection?>()
            val worker = launch(Dispatchers.IO) {
                try {
                    val boundary = "relay-${UUID.randomUUID()}"
                    val filename = name.replace(Regex("[^A-Za-z0-9._-]"), "_")
                    val prefix = ("--$boundary\r\nContent-Disposition: form-data; name=\"file\"; filename=\"$filename\"\r\n" +
                        "Content-Type: application/zip\r\n\r\n").toByteArray(Charsets.UTF_8)
                    val suffix = "\r\n--$boundary--\r\n".toByteArray(Charsets.US_ASCII)
                    val connection = connections.open(url)
                    active.set(connection)
                    if (!continuation.isActive) throw CancellationException()
                    connection.apply {
                        requestMethod = "POST"
                        connectTimeout = 15_000; readTimeout = 60_000
                        instanceFollowRedirects = false; useCaches = false; doOutput = true
                        setRequestProperty("User-Agent", UrlConnectionHttp.userAgent(version))
                        setRequestProperty("Content-Type", "multipart/form-data; boundary=$boundary")
                        setFixedLengthStreamingMode(prefix.size.toLong() + file.length() + suffix.size)
                    }
                    connection.outputStream.use { output ->
                        output.write(prefix)
                        file.inputStream().use { input ->
                            val buffer = ByteArray(8192)
                            while (true) {
                                if (!continuation.isActive) throw CancellationException()
                                val size = input.read(buffer)
                                if (size < 0) break
                                output.write(buffer, 0, size)
                            }
                        }
                        output.write(suffix)
                    }
                    val status = connection.responseCode
                    if (status != 200) throw LogUploadFailure("http", status)
                    val result = try {
                        val body = connection.inputStream.use { UrlConnectionHttp.readBounded(it, 64 * 1024) }
                        val json = JSONObject(String(body, Charsets.UTF_8))
                        val encoded = json.optString("downloadLinkEncoded").takeIf { it.isNotBlank() }
                        val link = encoded ?: json.optString("downloadLink")
                        val parsed = java.net.URI(link)
                        if (parsed.scheme != "https" || parsed.host.isNullOrEmpty() || parsed.userInfo != null) throw IllegalArgumentException()
                        UploadResult(link)
                    } catch (_: Exception) { throw LogUploadFailure("invalid-response", status) }
                    if (continuation.isActive) continuation.resume(result)
                } catch (failure: Exception) {
                    if (continuation.isActive) continuation.resumeWithException(when (failure) {
                        is LogUploadFailure -> failure
                        else -> LogUploadFailure("upload-failed")
                    })
                } finally { active.getAndSet(null)?.disconnect() }
            }
            continuation.invokeOnCancellation { active.getAndSet(null)?.disconnect(); worker.cancel() }
        }
    }
    companion object { const val MAX_FILE_BYTES = 100_000_000L }
}
