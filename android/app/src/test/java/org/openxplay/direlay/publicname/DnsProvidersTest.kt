// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-20
package org.openxplay.direlay.publicname

import android.app.Application
import org.json.JSONArray
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import org.openxplay.direlay.publicname.FakeHttpServer.Response
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import java.io.IOException

/** Request URL, headers and body, and the response mapping, against an in-process HTTP server. */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [28], manifest = Config.NONE, application = Application::class)
class DnsProvidersTest {
    private val http = UrlConnectionHttp(UrlConnectionHttp.userAgent("9.9.9"), httpsOnly = false)
    private val duckToken = "a1b2c3d4-0000-0000-0000-000000000000"

    private fun failure(block: () -> Unit): PublicNameFailure =
        try { block(); throw AssertionError("expected a failure") } catch (failure: PublicNameFailure) { failure }

    private fun duck(server: FakeHttpServer) = dnsProvider(PublicNameConfig(true, DnsProviderId.DUCKDNS, "myrelay.duckdns.org",
        duckToken, true), http, ProviderEndpoints(duckdns = server.base, dynv6 = "https://unused.invalid"), clock = { 0L })

    private fun dynv6(server: FakeHttpServer, token: String = "tok+en/=&x") = dnsProvider(PublicNameConfig(true, DnsProviderId.DYNV6,
        "myrelay.dynv6.net", token, true), http, ProviderEndpoints(duckdns = "https://unused.invalid", dynv6 = server.base), clock = { 0L })

    @Test fun duckDnsAddressAndTxtRequests() {
        FakeHttpServer { Response(200, "OK") }.use { server ->
            val provider = duck(server)
            provider.updateAddress("192.168.43.1")
            val record = provider.setChallenge("DIdSjxuYjuLoVqZ0_dijyCAqIKNMmbqo8G8vSfjtfKI")
            provider.clearChallenge(record)
            val (update, set, clear) = server.requests
            assertEquals(listOf("GET", "GET", "GET"), server.requests.map { it.method })
            assertEquals("/update", update.path)
            assertEquals(mapOf("domains" to "myrelay", "token" to duckToken, "ip" to "192.168.43.1"), update.query)
            assertEquals(mapOf("domains" to "myrelay", "token" to duckToken, "txt" to "DIdSjxuYjuLoVqZ0_dijyCAqIKNMmbqo8G8vSfjtfKI"), set.query)
            assertEquals(mapOf("domains" to "myrelay", "token" to duckToken, "txt" to "", "clear" to "true"), clear.query)
            assertEquals("OpenXPlay-Relay/9.9.9 (Android)", update.header("User-Agent"))
            assertEquals((1..9).map { "ns$it.duckdns.org" }, provider.nameServers)
        }
    }

    @Test fun duckDnsKoAndHttpStatusesMapToSharedErrors() {
        FakeHttpServer { Response(200, "KO") }.use { assertEquals(PublicNameError.TOKEN_REJECTED, failure { duck(it).updateAddress("10.0.0.2") }.error) }
        FakeHttpServer { Response(200, "maintenance") }.use { assertEquals(PublicNameError.PROVIDER_UNREACHABLE, failure { duck(it).updateAddress("10.0.0.2") }.error) }
        FakeHttpServer { Response(401, "") }.use { assertEquals(PublicNameError.TOKEN_REJECTED, failure { duck(it).updateAddress("10.0.0.2") }.error) }
        FakeHttpServer { Response(403, "") }.use { assertEquals(PublicNameError.TOKEN_REJECTED, failure { duck(it).updateAddress("10.0.0.2") }.error) }
        FakeHttpServer { Response(404, "") }.use { assertEquals(PublicNameError.NAME_NOT_FOUND, failure { duck(it).updateAddress("10.0.0.2") }.error) }
        FakeHttpServer { Response(500, "") }.use { assertEquals(PublicNameError.PROVIDER_UNREACHABLE, failure { duck(it).updateAddress("10.0.0.2") }.error) }
        FakeHttpServer { Response(429, "", mapOf("Retry-After" to "120")) }.use {
            val limited = failure { duck(it).updateAddress("10.0.0.2") }
            assertEquals(PublicNameError.PROVIDER_RATE_LIMITED, limited.error)
            assertEquals(120_000L, limited.retryAfterMillis)
        }
        // Redirects are not followed: the token must reach only the host the request was built for.
        FakeHttpServer { Response(302, "", mapOf("Location" to "http://127.0.0.1:9/elsewhere")) }.use { server ->
            assertEquals(PublicNameError.PROVIDER_UNREACHABLE, failure { duck(server).updateAddress("10.0.0.2") }.error)
            assertEquals(1, server.requests.size)
        }
        val closed = FakeHttpServer { Response(200, "OK") }.also { it.close() }
        assertEquals(PublicNameError.PROVIDER_UNREACHABLE, failure { duck(closed).updateAddress("10.0.0.2") }.error)
    }

    @Test fun dynv6UpdateEncodesEveryParameter() {
        FakeHttpServer { Response(200, "addresses updated") }.use { server ->
            dynv6(server).updateAddress("172.16.0.5")
            val request = server.requests.single()
            assertEquals("/api/update", request.path)
            assertEquals(mapOf("zone" to "myrelay.dynv6.net", "token" to "tok+en/=&x", "ipv4" to "172.16.0.5"), request.query)
            assertTrue("reserved characters are percent-encoded", request.rawQuery!!.contains("token=tok%2Ben%2F%3D%26x"))
            assertNull(request.header("Authorization"))
        }
        // Any 2xx is success; the wording of the body does not matter.
        FakeHttpServer { Response(200, "addresses unchanged") }.use { dynv6(it).updateAddress("172.16.0.5") }
        FakeHttpServer { Response(401, "invalid authentication token") }.use {
            assertEquals(PublicNameError.TOKEN_REJECTED, failure { dynv6(it).updateAddress("172.16.0.5") }.error)
        }
        FakeHttpServer { Response(404, "zone not found") }.use {
            assertEquals(PublicNameError.NAME_NOT_FOUND, failure { dynv6(it).updateAddress("172.16.0.5") }.error)
        }
    }

    @Test fun dynv6ChallengeReplacesStaleRecordsAndClearsItsOwn() {
        FakeHttpServer { request ->
            when ("${request.method} ${request.path}") {
                "GET /api/v2/zones/by-name/myrelay.dynv6.net" -> Response(200, """{"id":4711,"name":"myrelay.dynv6.net"}""")
                "GET /api/v2/zones/4711/records" -> Response(200, JSONArray()
                    .put(JSONObject().put("id", 1).put("type", "TXT").put("name", "_acme-challenge").put("data", "old"))
                    .put(JSONObject().put("id", 2).put("type", "A").put("name", "").put("data", "192.168.1.2"))
                    .put(JSONObject().put("id", 3).put("type", "TXT").put("name", "other").put("data", "keep")).toString())
                "DELETE /api/v2/zones/4711/records/1", "DELETE /api/v2/zones/4711/records/99" -> Response(204)
                "POST /api/v2/zones/4711/records" -> Response(200, """{"id":99,"type":"TXT","name":"_acme-challenge","data":"v"}""")
                else -> Response(500, "unexpected")
            }
        }.use { server ->
            val provider = dynv6(server, "rest-token")
            val record = provider.setChallenge("txt-value")
            assertEquals(ChallengeRecord("4711", "99"), record)
            provider.clearChallenge(record)
            assertEquals(listOf("GET /api/v2/zones/by-name/myrelay.dynv6.net", "GET /api/v2/zones/4711/records",
                "DELETE /api/v2/zones/4711/records/1", "POST /api/v2/zones/4711/records", "DELETE /api/v2/zones/4711/records/99"),
                server.requests.map { "${it.method} ${it.path}" })
            assertTrue(server.requests.all { it.header("Authorization") == "Bearer rest-token" })
            val post = server.requests[3]
            assertEquals("application/json", post.header("Content-Type"))
            val body = JSONObject(post.text)
            assertEquals(setOf("type", "name", "data"), body.keys().asSequence().toSet())
            assertEquals("TXT", body.getString("type"))
            assertEquals("_acme-challenge", body.getString("name"))
            assertEquals("txt-value", body.getString("data"))
            assertEquals(listOf("ns1.dynv6.com", "ns2.dynv6.com", "ns3.dynv6.com"), provider.nameServers)
        }
        // Ids go into URL paths, so anything but a plain identifier is refused.
        FakeHttpServer { Response(200, """{"id":"../x"}""") }.use {
            assertEquals(PublicNameError.PROVIDER_UNREACHABLE, failure { dynv6(it).setChallenge("v") }.error)
        }
        FakeHttpServer { Response(404, "") }.use { assertEquals(PublicNameError.NAME_NOT_FOUND, failure { dynv6(it).setChallenge("v") }.error) }
    }

    @Test fun clientRefusesPlainHttpCapsBodiesAndParsesRetryAfter() {
        val strict = UrlConnectionHttp(UrlConnectionHttp.userAgent("1"))
        assertThrows(IOException::class.java) { strict.send(HttpRequest("GET", "http://127.0.0.1:9/")) }
        FakeHttpServer { Response(200, ByteArray(UrlConnectionHttp.MAX_BODY_BYTES + 1) { 'a'.code.toByte() }) }.use { server ->
            assertThrows(IOException::class.java) { http.send(HttpRequest("GET", server.base + "/")) }
        }
        FakeHttpServer { Response(200, ByteArray(UrlConnectionHttp.MAX_BODY_BYTES) { 'a'.code.toByte() }) }.use { server ->
            assertEquals(UrlConnectionHttp.MAX_BODY_BYTES, http.send(HttpRequest("GET", server.base + "/")).body.size)
        }
        assertEquals(15_000, UrlConnectionHttp.TIMEOUT_MS)
        assertEquals(5_000L, retryAfterMillis("5", 0))
        assertEquals(60_000L, retryAfterMillis("Wed, 21 Oct 2026 07:29:00 GMT", 1792567680000L))
        assertNull(retryAfterMillis("soon", 0))
        assertNull(retryAfterMillis(null, 0))
    }
}
