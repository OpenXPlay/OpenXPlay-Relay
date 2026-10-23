// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-22
package org.openxplay.direlay.logs

import android.app.Application
import kotlinx.coroutines.async
import kotlinx.coroutines.cancelAndJoin
import kotlinx.coroutines.runBlocking
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
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [33], application = Application::class)
class LogUploaderTest {
    @get:Rule val temporary = TemporaryFolder()
    private open class Connection(private val code: Int = 200,
        private val body: String = "{\"downloadLinkEncoded\":\"https://d1.tfdl.net/example%20file\"}") : HttpURLConnection(URL("https://test.invalid/api/upload")) {
        val sent = ByteArrayOutputStream()
        var length = -1L
        var disconnected = false
        override fun connect() = Unit
        override fun usingProxy() = false
        override fun disconnect() { disconnected = true }
        override fun getOutputStream() = sent
        override fun getInputStream() = ByteArrayInputStream(body.toByteArray())
        override fun getResponseCode() = code
        override fun setFixedLengthStreamingMode(contentLength: Long) { length = contentLength }
    }
    private fun archive(): File = temporary.newFile("logs.zip").apply { writeBytes(byteArrayOf(0x50, 0x4b, 0, 1)) }
    private suspend fun failure(uploader: LogUploader, file: File): LogUploadFailure {
        try { uploader.upload(file, file.name); fail("Expected an upload failure") }
        catch (failure: LogUploadFailure) { return failure }
        throw AssertionError("Expected an upload failure")
    }
    @Test fun multipartUsesOneFileFieldAndFixedLengthWithRequiredHeaders() = runBlocking {
        val connection = Connection()
        val uploader = TmpfileLinkUploader(UploadConnectionFactory { url ->
            assertEquals("https://test.invalid/api/upload", url.toString()); connection
        }, "1.2.3", "https://test.invalid")
        val result = uploader.upload(archive(), "logs.zip")
        assertEquals("https://d1.tfdl.net/example%20file", result.link)
        assertEquals("POST", connection.requestMethod)
        assertEquals(15_000, connection.connectTimeout); assertEquals(60_000, connection.readTimeout)
        assertFalse(connection.instanceFollowRedirects)
        assertEquals("OpenXPlay-Relay/1.2.3 (Android)", connection.getRequestProperty("User-Agent"))
        val boundary = connection.getRequestProperty("Content-Type").substringAfter("boundary=")
        val body = connection.sent.toString("UTF-8")
        assertTrue(body.startsWith("--$boundary\r\n"))
        assertTrue(body.contains("Content-Disposition: form-data; name=\"file\"; filename=\"logs.zip\"\r\n"))
        assertTrue(body.contains("Content-Type: application/zip\r\n\r\nPK"))
        assertTrue(body.endsWith("\r\n--$boundary--\r\n"))
        assertEquals(connection.sent.size().toLong(), connection.length)
        assertTrue(connection.disconnected)
        assertFalse(result.toString().contains("tfdl.net"))
    }
    @Test fun fallbackLinkIsAcceptedWhenEncodedLinkIsAbsent() = runBlocking {
        val file = archive()
        for (body in listOf("{\"downloadLink\":\"https://example.org/file\"}",
            "{\"downloadLinkEncoded\":\"\",\"downloadLink\":\"https://example.org/file\"}")) {
            val result = TmpfileLinkUploader(UploadConnectionFactory { Connection(body = body) }, "1").upload(file, "logs.zip")
            assertEquals("https://example.org/file", result.link)
        }
    }
    @Test fun nonHttpsMissingAndMalformedLinksAreRejectedWithTheStatusOnly() = runBlocking {
        val file = archive()
        for (body in listOf("{\"downloadLinkEncoded\":\"http://example.org/file\"}", "{}", "not json",
            "{\"downloadLinkEncoded\":\"https://u:p@example.org/private\"}")) {
            val error = failure(TmpfileLinkUploader(UploadConnectionFactory { Connection(body = body) }, "1"), file)
            assertEquals("invalid-response", error.reason); assertEquals(200, error.status)
            assertFalse(error.message.orEmpty().contains("example.org"))
        }
    }
    @Test fun httpErrorsNeverExposeTheResponseBody() = runBlocking {
        val error = failure(TmpfileLinkUploader(UploadConnectionFactory { Connection(503, "private response") }, "1"), archive())
        assertEquals("http", error.reason); assertEquals(503, error.status)
        assertFalse(error.message.orEmpty().contains("private response"))
    }
    @Test fun responseBodiesAreCappedAt64Kib() = runBlocking {
        val error = failure(TmpfileLinkUploader(UploadConnectionFactory { Connection(body = "x".repeat(65_537)) }, "1"), archive())
        assertEquals("invalid-response", error.reason); assertEquals(200, error.status)
    }
    @Test fun oversizeArchivesAreRefusedBeforeAnyConnection() = runBlocking {
        val file = archive()
        java.io.RandomAccessFile(file, "rw").use { it.setLength(100_000_001) }
        val error = failure(TmpfileLinkUploader(UploadConnectionFactory { throw AssertionError("Should not connect") }, "1"), file)
        assertEquals("too-large", error.reason)
    }
    @Test fun insecureUploadEndpointsAreRefusedBeforeConnecting() = runBlocking {
        val error = failure(TmpfileLinkUploader(UploadConnectionFactory { throw AssertionError("Should not connect") }, "1", "http://test.invalid"), archive())
        assertEquals("https-required", error.reason)
    }
    @Test fun missingValidatedInternetNetworkIsReportedWithoutARequest() = runBlocking {
        val error = failure(TmpfileLinkUploader(UploadConnectionFactory { throw LogUploadFailure("no-internet") }, "1"), archive())
        assertEquals("no-internet", error.reason)
    }
    @Test fun cancellationDisconnectsABlockedConnection() = runBlocking {
        val started = CountDownLatch(1)
        val stopped = CountDownLatch(1)
        val connection = object : Connection() {
            override fun getInputStream(): ByteArrayInputStream {
                started.countDown()
                check(stopped.await(5, TimeUnit.SECONDS))
                throw IOException("cancelled")
            }
            override fun disconnect() { super.disconnect(); stopped.countDown() }
        }
        val file = archive()
        val upload = async(kotlinx.coroutines.Dispatchers.Default) {
            TmpfileLinkUploader(UploadConnectionFactory { connection }, "1").upload(file, file.name)
        }
        assertTrue(started.await(5, TimeUnit.SECONDS))
        upload.cancelAndJoin()
        assertTrue(connection.disconnected)
    }
}
