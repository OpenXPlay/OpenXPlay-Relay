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
import kotlinx.coroutines.withTimeoutOrNull
import org.json.JSONObject
import org.openxplay.direlay.publicname.UrlConnectionHttp
import java.io.File
import java.net.HttpURLConnection
import java.net.URL
import java.security.MessageDigest
import java.util.UUID
import java.util.concurrent.atomic.AtomicReference
import kotlin.coroutines.resume
import kotlin.coroutines.resumeWithException

interface LogUploader {
    // Progress describes transmission; only the returned result confirms a validated server receipt.
    suspend fun upload(file: File, name: String, onProgress: suspend (LogUploadProgress) -> Unit = {}): UploadResult
    fun latestReceipt(): UploadResult? = null
}
sealed interface LogUploadProgress {
    data object Connecting : LogUploadProgress
    data class Sending(val bytesSent: Long, val totalBytes: Long) : LogUploadProgress
    data object AwaitingReceipt : LogUploadProgress
}
class LogUploadFailure(val reason: String, val status: Int? = null, val retryAfterSeconds: Long? = null) :
    Exception(if (status == null) reason else "$reason (HTTP $status)")
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

class OpenXPlayLogUploader(private val connections: UploadConnectionFactory, private val version: String,
    private val receipts: LogUploadStore, private val baseUrl: String = LOG_UPLOAD_BASE_URL) : LogUploader {
    override fun latestReceipt() = receipts.latest()

    override suspend fun upload(file: File, name: String, onProgress: suspend (LogUploadProgress) -> Unit): UploadResult = withTimeoutOrNull(180_000) { coroutineScope {
        val totalBytes = file.length()
        if (!file.isFile || totalBytes == 0L) throw LogUploadFailure("empty")
        if (totalBytes > MAX_FILE_BYTES) throw LogUploadFailure("too-large")
        val origin = URL(baseUrl)
        if (origin.protocol != "https") throw LogUploadFailure("https-required")
        if (origin.userInfo != null || origin.query != null || origin.ref != null || origin.path.trim('/').isNotEmpty()) {
            throw LogUploadFailure("invalid-endpoint")
        }
        val publicUrl = baseUrl.trimEnd('/')
        val url = URL("$publicUrl/api/uploads")
        suspendCancellableCoroutine { continuation ->
            val active = AtomicReference<HttpURLConnection?>()
            val worker = launch(Dispatchers.IO) {
                suspend fun report(progress: LogUploadProgress) {
                    if (!continuation.isActive) throw CancellationException()
                    onProgress(progress)
                }
                try {
                    report(LogUploadProgress.Connecting)
                    val userId = receipts.userId()
                    if (!validLogUserId(userId)) throw LogUploadFailure("invalid-user-id")
                    if (version.toByteArray(Charsets.UTF_8).size > 512) throw LogUploadFailure("invalid-version")
                    val boundary = "relay-${UUID.randomUUID()}"
                    val filename = name.replace(Regex("[^A-Za-z0-9._-]"), "_")
                    fun field(key: String, value: String) = "--$boundary\r\nContent-Disposition: form-data; name=\"$key\"\r\n\r\n$value\r\n"
                    val prefix = (field("user_id", userId) + field("version", version) +
                        "--$boundary\r\nContent-Disposition: form-data; name=\"file\"; filename=\"$filename\"\r\n" +
                        "Content-Type: application/zip\r\n\r\n").toByteArray(Charsets.UTF_8)
                    val suffix = "\r\n--$boundary--\r\n".toByteArray(Charsets.US_ASCII)
                    val connection = connections.open(url)
                    active.set(connection)
                    if (!continuation.isActive) throw CancellationException()
                    connection.apply {
                        requestMethod = "POST"
                        connectTimeout = 30_000; readTimeout = 180_000
                        instanceFollowRedirects = false; useCaches = false; doOutput = true
                        setRequestProperty("User-Agent", UrlConnectionHttp.userAgent(version))
                        setRequestProperty("Content-Type", "multipart/form-data; boundary=$boundary")
                        setFixedLengthStreamingMode(prefix.size.toLong() + totalBytes + suffix.size)
                    }
                    val digest = MessageDigest.getInstance("SHA-256")
                    var sent = 0L
                    var reported = 0L
                    connection.outputStream.use { output ->
                        output.write(prefix)
                        report(LogUploadProgress.Sending(0, totalBytes))
                        file.inputStream().use { input ->
                            val buffer = ByteArray(8192)
                            while (true) {
                                if (!continuation.isActive) throw CancellationException()
                                val size = input.read(buffer)
                                if (size < 0) break
                                output.write(buffer, 0, size)
                                digest.update(buffer, 0, size)
                                sent += size
                                // Limit UI updates to roughly one per percent for large files.
                                if (sent == totalBytes || sent - reported >= maxOf(8192L, totalBytes / 100)) {
                                    report(LogUploadProgress.Sending(sent, totalBytes))
                                    reported = sent
                                }
                            }
                        }
                        output.write(suffix)
                    }
                    report(LogUploadProgress.AwaitingReceipt)
                    val status = connection.responseCode
                    if (status != 201) throw LogUploadFailure("http", status,
                        connection.getHeaderField("Retry-After")?.toLongOrNull()?.takeIf { it >= 0 }?.coerceAtMost(86_400))
                    val result = try {
                        val body = connection.inputStream.use { UrlConnectionHttp.readBounded(it, 64 * 1024) }
                        val json = JSONObject(String(body, Charsets.UTF_8))
                        check(json.getString("user_id") == userId)
                        UploadResult.fromJson(json, publicUrl).also {
                            check(it.bytes == sent)
                            check(it.sha256 == digest.digest().joinToString("") { byte -> "%02x".format(byte) })
                        }
                    } catch (_: Exception) { throw LogUploadFailure("invalid-response", status) }
                    // A valid server receipt remains useful even if local preferences cannot be written.
                    runCatching { receipts.save(result) }
                    if (continuation.isActive) continuation.resume(result)
                } catch (failure: Exception) {
                    if (continuation.isActive) continuation.resumeWithException(when (failure) {
                        is LogUploadFailure -> failure
                        else -> LogUploadFailure("unconfirmed")
                    })
                } finally { active.getAndSet(null)?.disconnect() }
            }
            continuation.invokeOnCancellation { active.getAndSet(null)?.disconnect(); worker.cancel() }
        }
    } } ?: throw LogUploadFailure("unconfirmed")
    companion object { const val MAX_FILE_BYTES = LOG_UPLOAD_MAX_FILE_BYTES }
}
