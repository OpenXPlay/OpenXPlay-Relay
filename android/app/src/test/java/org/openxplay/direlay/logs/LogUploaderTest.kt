// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-22
package org.openxplay.direlay.logs

import android.app.Application
import kotlinx.coroutines.async
import kotlinx.coroutines.cancelAndJoin
import kotlinx.coroutines.runBlocking
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import java.io.ByteArrayInputStream
import java.io.ByteArrayOutputStream
import java.io.File
import java.io.IOException
import java.net.HttpURLConnection
import java.net.URL
import java.security.MessageDigest
import java.util.concurrent.CountDownLatch
import java.util.concurrent.CopyOnWriteArrayList
import java.util.concurrent.TimeUnit

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [33], application = Application::class)
class LogUploaderTest {
    @get:Rule val temporary = TemporaryFolder()
    private val payload = byteArrayOf(0x50, 0x4b, 0, 1, -1)
    private val userId = "installation-user-01"
    private val id = "0123456789abcdef0123456789abcdef"
    private fun receipt() = JSONObject().put("id", id).put("user_id", userId)
        .put("tracking_url", "$LOG_UPLOAD_BASE_URL/r/$id").put("qr_code_url", "$LOG_UPLOAD_BASE_URL/qr/$id.png")
        .put("expires_at", "2026-11-23T12:34:56.123456789Z")
        .put("bytes", payload.size).put("sha256", MessageDigest.getInstance("SHA-256").digest(payload).joinToString("") { "%02x".format(it) })
    private class Store(private val id: String = "installation-user-01") : LogUploadStore {
        var saved: UploadResult? = null
        override fun userId() = id
        override fun latest() = saved
        override fun save(receipt: UploadResult) { saved = receipt }
    }
    private open class Connection(private val code: Int = 201, private val body: String,
        private val retryAfter: String? = null) : HttpURLConnection(URL("$LOG_UPLOAD_BASE_URL/api/uploads")) {
        val sent = ByteArrayOutputStream()
        var length = -1L
        var disconnected = false
        override fun connect() = Unit
        override fun usingProxy() = false
        override fun disconnect() { disconnected = true }
        override fun getOutputStream() = sent
        override fun getInputStream() = ByteArrayInputStream(body.toByteArray())
        override fun getResponseCode() = code
        override fun getHeaderField(name: String?) = if (name == "Retry-After") retryAfter else null
        override fun setFixedLengthStreamingMode(contentLength: Long) { length = contentLength }
    }
    private fun archive(): File = temporary.newFile("logs-${java.util.UUID.randomUUID()}.zip").apply { writeBytes(payload) }
    private fun uploader(connection: Connection, store: Store = Store()) = OpenXPlayLogUploader(UploadConnectionFactory { connection }, "1.2.3", store)
    private suspend fun failure(uploader: LogUploader, file: File): LogUploadFailure {
        try { uploader.upload(file, file.name); fail("Expected an upload failure") }
        catch (failure: LogUploadFailure) { return failure }
        throw AssertionError("Expected an upload failure")
    }
    @Test fun multipartPreservesFileBytesAndSendsStableUserAndVersionToTheCanonicalApi() = runBlocking {
        val connection = Connection(body = receipt().toString())
        val store = Store()
        val client = OpenXPlayLogUploader(UploadConnectionFactory { url ->
            assertEquals("$LOG_UPLOAD_BASE_URL/api/uploads", url.toString()); connection
        }, "1.2.3", store)
        val result = client.upload(archive(), "logs.zip")
        assertEquals(id, result.id); assertEquals("$LOG_UPLOAD_BASE_URL/r/$id", result.link)
        assertEquals(30_000, connection.connectTimeout); assertEquals(180_000, connection.readTimeout)
        assertEquals("POST", connection.requestMethod); assertFalse(connection.instanceFollowRedirects)
        assertEquals("OpenXPlay-Relay/1.2.3 (Android)", connection.getRequestProperty("User-Agent"))
        val boundary = connection.getRequestProperty("Content-Type").substringAfter("boundary=")
        val prefix = ("--$boundary\r\nContent-Disposition: form-data; name=\"user_id\"\r\n\r\n$userId\r\n" +
            "--$boundary\r\nContent-Disposition: form-data; name=\"version\"\r\n\r\n1.2.3\r\n" +
            "--$boundary\r\nContent-Disposition: form-data; name=\"file\"; filename=\"logs.zip\"\r\nContent-Type: application/zip\r\n\r\n").toByteArray()
        assertArrayEquals(prefix + payload + "\r\n--$boundary--\r\n".toByteArray(), connection.sent.toByteArray())
        assertEquals(connection.sent.size().toLong(), connection.length)
        assertSame(result, store.saved); assertSame(result, client.latestReceipt())
        assertTrue(connection.disconnected); assertFalse(result.toString().contains(id))
    }
    @Test fun sameUserCanUploadAgainAndReceivesAnIndependentReceipt() = runBlocking {
        val store = Store()
        val ids = listOf(id, "abcdef0123456789abcdef0123456789")
        var calls = 0
        val client = OpenXPlayLogUploader(UploadConnectionFactory {
            val next = ids[calls++]
            Connection(body = receipt().put("id", next).put("tracking_url", "$LOG_UPLOAD_BASE_URL/r/$next")
                .put("qr_code_url", "$LOG_UPLOAD_BASE_URL/qr/$next.png").toString())
        }, "1", store)
        val file = archive()
        assertNotEquals(client.upload(file, "logs.zip").id, client.upload(file, "logs.zip").id)
        assertEquals(2, calls); assertEquals(ids.last(), store.saved?.id)
    }
    @Test fun receiptMustMatchTheUserFileIdHashAndPinnedReceiptUrls() = runBlocking {
        val invalid = listOf(
            receipt().put("user_id", "other-user"), receipt().put("id", "invalid"),
            receipt().put("bytes", payload.size + 1), receipt().put("sha256", "0".repeat(64)),
            receipt().put("tracking_url", "https://example.org/r/$id"),
            receipt().put("tracking_url", "$LOG_UPLOAD_BASE_URL/r/$id?redirect=elsewhere"),
            receipt().put("qr_code_url", "http://logs.openxplay.org/qr/$id.png"),
            receipt().put("expires_at", "not a date"), receipt().put("expires_at", "+999999999-01-01T00:00:00Z"), JSONObject())
        for (json in invalid) {
            val store = Store()
            val error = failure(uploader(Connection(body = json.toString()), store), archive())
            assertEquals("invalid-response", error.reason); assertEquals(201, error.status)
            assertNull(store.saved); assertFalse(error.message.orEmpty().contains(userId))
        }
    }
    @Test fun onlyCreatedIsSuccessAndRejectionsNeverRetryOrExposeResponseBodies() = runBlocking {
        for (status in listOf(200, 301, 400, 403, 429, 503, 507)) {
            var calls = 0
            val connection = Connection(status, "private response", "60")
            val store = Store()
            val client = OpenXPlayLogUploader(UploadConnectionFactory { calls++; connection }, "1", store)
            val error = failure(client, archive())
            assertEquals("http", error.reason); assertEquals(status, error.status)
            assertEquals(60L, error.retryAfterSeconds); assertEquals(1, calls)
            assertNull(store.saved); assertTrue(connection.disconnected)
            assertFalse(error.message.orEmpty().contains("private response"))
        }
    }
    @Test fun malformedAndOversizedResponsesLeaveTheUploadUnconfirmed() = runBlocking {
        for (body in listOf("not json", "x".repeat(65_537))) {
            val error = failure(uploader(Connection(body = body)), archive())
            assertEquals("invalid-response", error.reason); assertEquals(201, error.status)
        }
    }
    @Test fun emptyAndOversizeArchivesAreRefusedBeforeAnyConnection() = runBlocking {
        val file = archive()
        val client = OpenXPlayLogUploader(UploadConnectionFactory { throw AssertionError("Should not connect") }, "1", Store())
        file.writeBytes(byteArrayOf())
        assertEquals("empty", failure(client, file).reason)
        java.io.RandomAccessFile(file, "rw").use { it.setLength(52_428_801) }
        assertEquals("too-large", failure(client, file).reason)
    }
    @Test fun insecureEndpointsAndInvalidUserIdsAreRefusedBeforeConnecting() = runBlocking {
        val connections = UploadConnectionFactory { throw AssertionError("Should not connect") }
        assertEquals("https-required", failure(OpenXPlayLogUploader(connections, "1", Store(), "http://test.invalid"), archive()).reason)
        assertEquals("invalid-user-id", failure(OpenXPlayLogUploader(connections, "1", Store(" invalid ")), archive()).reason)
    }
    @Test fun missingValidatedInternetNetworkIsReportedWithoutARequest() = runBlocking {
        val error = failure(OpenXPlayLogUploader(UploadConnectionFactory { throw LogUploadFailure("no-internet") }, "1", Store()), archive())
        assertEquals("no-internet", error.reason)
    }
    @Test fun cancellationDisconnectsABlockedConnectionAndKeepsTheLocalArchive() = runBlocking {
        val started = CountDownLatch(1)
        val stopped = CountDownLatch(1)
        val connection = object : Connection(body = receipt().toString()) {
            override fun getInputStream(): ByteArrayInputStream {
                started.countDown(); check(stopped.await(5, TimeUnit.SECONDS)); throw IOException("cancelled")
            }
            override fun disconnect() { super.disconnect(); stopped.countDown() }
        }
        val file = archive()
        val upload = async(kotlinx.coroutines.Dispatchers.Default) { uploader(connection).upload(file, "logs.zip") }
        assertTrue(started.await(5, TimeUnit.SECONDS))
        upload.cancelAndJoin()
        assertTrue(connection.disconnected); assertArrayEquals(payload, file.readBytes())
    }
    @Test fun progressCountsOnlyWrittenFileBytesAndWaitsForAValidatedReceipt() = runBlocking {
        val data = ByteArray(100_003) { it.toByte() }
        val file = archive().apply { writeBytes(data) }
        val events = mutableListOf<LogUploadProgress>()
        val store = Store()
        var closed = false
        val connection = object : Connection(body = receipt().put("bytes", data.size)
            .put("sha256", MessageDigest.getInstance("SHA-256").digest(data).joinToString("") { "%02x".format(it) }).toString()) {
            private val output = object : ByteArrayOutputStream() {
                override fun close() { closed = true; super.close() }
            }
            override fun getOutputStream() = output
            override fun getResponseCode(): Int {
                assertTrue(closed)
                assertEquals(LogUploadProgress.AwaitingReceipt, events.last())
                assertNull(store.saved)
                return super.getResponseCode()
            }
        }
        val result = uploader(connection, store).upload(file, "logs.zip") { events += it }
        assertEquals(LogUploadProgress.Connecting, events.first())
        assertEquals(LogUploadProgress.AwaitingReceipt, events.last())
        val sending = events.filterIsInstance<LogUploadProgress.Sending>()
        assertEquals(0L, sending.first().bytesSent)
        assertEquals(data.size.toLong(), sending.last().bytesSent)
        assertTrue(sending.all { it.totalBytes == data.size.toLong() && it.bytesSent in 0..it.totalBytes })
        assertTrue(sending.zipWithNext().all { (before, after) -> after.bytesSent > before.bytesSent })
        assertSame(result, store.saved)
    }
    @Test fun cancellingDuringSendingNeverReportsFullTransferOrReceiptWaiting() = runBlocking {
        val blocked = CountDownLatch(1)
        val stopped = CountDownLatch(1)
        val events = CopyOnWriteArrayList<LogUploadProgress>()
        val connection = object : Connection(body = receipt().toString()) {
            private val output = object : ByteArrayOutputStream() {
                private var writes = 0
                override fun write(data: ByteArray, offset: Int, size: Int) {
                    if (++writes == 3) {
                        blocked.countDown(); check(stopped.await(5, TimeUnit.SECONDS)); throw IOException("cancelled")
                    }
                    super.write(data, offset, size)
                }
            }
            override fun getOutputStream() = output
            override fun disconnect() { super.disconnect(); stopped.countDown() }
        }
        val file = archive().apply { writeBytes(ByteArray(32_768)) }
        val upload = async(kotlinx.coroutines.Dispatchers.Default) { uploader(connection).upload(file, "logs.zip") { events += it } }
        assertTrue(blocked.await(5, TimeUnit.SECONDS))
        assertEquals(LogUploadProgress.Sending(8192, file.length()), events.last())
        upload.cancelAndJoin()
        assertFalse(events.contains(LogUploadProgress.AwaitingReceipt))
        assertFalse(events.filterIsInstance<LogUploadProgress.Sending>().any { it.bytesSent == it.totalBytes })
        assertTrue(connection.disconnected); assertEquals(32_768L, file.length())
    }
}
