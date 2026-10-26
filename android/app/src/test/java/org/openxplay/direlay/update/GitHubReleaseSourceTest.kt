// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-25
package org.openxplay.direlay.update

import android.app.Application
import android.net.ConnectivityManager
import kotlinx.coroutines.*
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config
import java.io.ByteArrayInputStream
import java.io.IOException
import java.io.InputStream
import java.net.HttpURLConnection
import java.net.SocketTimeoutException
import java.net.URL
import java.time.Instant
import java.util.Locale
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicInteger

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [33], application = Application::class)
class GitHubReleaseSourceTest {
    private open class Connection(private val code: Int = 200, private val body: String = "[]",
        private val headers: Map<String, String> = emptyMap()) : HttpURLConnection(URL(GitHubReleaseSource.ENDPOINT)) {
        @Volatile var disconnected = false
        override fun connect() = Unit
        override fun usingProxy() = false
        override fun disconnect() { disconnected = true }
        override fun getInputStream(): InputStream = ByteArrayInputStream(body.toByteArray(Charsets.UTF_8))
        override fun getResponseCode() = code
        override fun getHeaderField(name: String?) = headers[name?.lowercase(Locale.ROOT)]
    }
    private fun source(connection: Connection, timeout: Long = 40_000) = GitHubReleaseSource(ReleaseConnectionFactory { connection }, "0.1.2", timeout)
    private suspend fun failure(source: ReleaseSource): UpdateFailure {
        try { source.fetch(); fail("Expected update failure") }
        catch (failure: UpdateFailure) { return failure }
        throw AssertionError("Expected update failure")
    }

    @Test fun oneGetSendsOnlyThePublicApiHeadersAndParsesTheFeed() = runBlocking {
        val connection = Connection(body = ReleaseFixtures.feed())
        var calls = 0
        val client = GitHubReleaseSource(ReleaseConnectionFactory { url ->
            assertEquals(GitHubReleaseSource.ENDPOINT, url.toString()); calls++; connection
        }, "0.1.2")
        assertEquals(4, client.fetch().size)
        assertEquals(1, calls); assertEquals("GET", connection.requestMethod)
        assertEquals(15_000, connection.connectTimeout); assertEquals(20_000, connection.readTimeout)
        assertFalse(connection.instanceFollowRedirects); assertFalse(connection.useCaches)
        assertEquals("application/vnd.github+json", connection.getRequestProperty("Accept"))
        assertEquals("2022-11-28", connection.getRequestProperty("X-GitHub-Api-Version"))
        assertEquals("OpenXPlay-Relay-Android/0.1.2", connection.getRequestProperty("User-Agent"))
        assertNull(connection.getRequestProperty("Authorization"))
        assertNull(connection.getRequestProperty("Cookie"))
        assertTrue(connection.disconnected)
    }
    @Test fun rateLimitsRequireBothTheStatusAndZeroRemainingAndKeepTheResetInstant() = runBlocking {
        val reset = Instant.parse("2026-10-25T13:00:00Z")
        for (status in listOf(403, 429)) {
            val connection = Connection(status, headers = mapOf("x-ratelimit-remaining" to "0", "x-ratelimit-reset" to reset.epochSecond.toString()))
            assertEquals(UpdateFailure.RateLimited(reset), failure(source(connection)))
            assertTrue(connection.disconnected)
            assertEquals(UpdateFailure.RateLimited(null), failure(source(Connection(status, headers = mapOf("x-ratelimit-remaining" to "0")))))
            assertEquals(UpdateFailure.RateLimited(null), failure(source(Connection(status, headers = mapOf("x-ratelimit-remaining" to "0", "x-ratelimit-reset" to "invalid")))))
        }
        for (status in listOf(403, 429, 404, 500)) assertEquals(UpdateFailure.Http(status), failure(source(Connection(status))))
    }
    @Test fun malformedWrongTypeAndOversizedResponsesAreInvalidAndAlwaysDisconnected() = runBlocking {
        val oversized = "[" + " ".repeat(GitHubReleaseSource.MAX_BODY_BYTES - 1) + "]"
        for (body in listOf("not json", "{}", "[1]", oversized)) {
            val connection = Connection(body = body)
            assertEquals(UpdateFailure.InvalidResponse, failure(source(connection)))
            assertTrue(connection.disconnected)
        }
        val exactlyAtCap = "[" + " ".repeat(GitHubReleaseSource.MAX_BODY_BYTES - 2) + "]"
        assertTrue(source(Connection(body = exactlyAtCap)).fetch().isEmpty())
        val oversizedHeader = object : Connection() {
            override fun getContentLengthLong() = GitHubReleaseSource.MAX_BODY_BYTES + 1L
            override fun getInputStream(): InputStream = throw AssertionError("Must reject length before reading")
        }
        assertEquals(UpdateFailure.InvalidResponse, failure(source(oversizedHeader)))
    }
    @Test fun socketTimeoutHasASpecificFailure() = runBlocking {
        val timedOut = object : Connection() { override fun getResponseCode(): Int = throw SocketTimeoutException() }
        assertEquals(UpdateFailure.Timeout, failure(source(timedOut))); assertTrue(timedOut.disconnected)
    }
    @Test fun otherConnectionErrorsMeanGitHubIsUnreachable() = runBlocking {
        val refused = object : Connection() { override fun getResponseCode(): Int = throw java.net.ConnectException() }
        assertEquals(UpdateFailure.Unreachable, failure(source(refused))); assertTrue(refused.disconnected)
        val unknownHost = object : Connection() { override fun getResponseCode(): Int = throw java.net.UnknownHostException("api.github.com") }
        assertEquals(UpdateFailure.Unreachable, failure(source(unknownHost)))
        val handshake = object : Connection() { override fun getResponseCode(): Int = throw javax.net.ssl.SSLHandshakeException("reset") }
        assertEquals(UpdateFailure.Unreachable, failure(source(handshake)))
    }
    @Test fun missingValidatedInternetIsReportedWithoutARequest() = runBlocking {
        val connectivity = RuntimeEnvironment.getApplication().getSystemService(ConnectivityManager::class.java)
        shadowOf(connectivity).clearAllNetworks()
        assertEquals(UpdateFailure.NoInternet, failure(GitHubReleaseSource(InternetReleaseConnection(connectivity), "0.1.2")))
    }
    @Test fun redirectsStayOnTheHttpsApiOriginAndNeverRetryHttpFailures() = runBlocking {
        for (target in listOf("http://api.github.com/releases", "https://example.org/releases", "https://api.github.com.evil.example/releases",
            "https://user@api.github.com/releases", "https://api.github.com:444/releases")) {
            var calls = 0
            val redirect = Connection(302, headers = mapOf("location" to target))
            val client = GitHubReleaseSource(ReleaseConnectionFactory { calls++; redirect }, "0.1.2")
            assertEquals(UpdateFailure.Http(302), failure(client)); assertEquals(1, calls); assertTrue(redirect.disconnected)
        }
        val redirect = Connection(307, headers = mapOf("location" to "/repos/OpenXPlay/OpenXPlay-Relay/releases?per_page=30"))
        val destination = Connection()
        var calls = 0
        val client = GitHubReleaseSource(ReleaseConnectionFactory { url ->
            assertEquals(GitHubReleaseSource.ENDPOINT, url.toString())
            if (calls++ == 0) redirect else destination
        }, "0.1.2")
        assertTrue(client.fetch().isEmpty()); assertEquals(2, calls)
        assertTrue(redirect.disconnected); assertTrue(destination.disconnected)
        calls = 0
        val failed = GitHubReleaseSource(ReleaseConnectionFactory { calls++; Connection(500) }, "0.1.2")
        assertEquals(UpdateFailure.Http(500), failure(failed)); assertEquals(1, calls)
    }
    @Test fun wholeCheckDeadlineDisconnectsEvenWhenTheResponseKeepsMakingProgress() = runBlocking {
        val stopped = CountDownLatch(1)
        val reads = AtomicInteger()
        val connection = object : Connection() {
            override fun getInputStream(): InputStream = object : InputStream() {
                override fun read(): Int {
                    reads.incrementAndGet()
                    if (stopped.await(10, TimeUnit.MILLISECONDS)) throw IOException("disconnected")
                    return ' '.code
                }
                override fun read(buffer: ByteArray, offset: Int, length: Int): Int {
                    buffer[offset] = read().toByte(); return 1
                }
            }
            override fun disconnect() { super.disconnect(); stopped.countDown() }
        }
        val start = System.nanoTime()
        assertEquals(UpdateFailure.Timeout, failure(source(connection, 500)))
        assertTrue(TimeUnit.NANOSECONDS.toMillis(System.nanoTime() - start) < 5_000)
        assertTrue(connection.disconnected); assertTrue(reads.get() > 0)
    }
}
