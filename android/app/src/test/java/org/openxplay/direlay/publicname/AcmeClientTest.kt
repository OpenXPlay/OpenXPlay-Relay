// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-20
package org.openxplay.direlay.publicname

import android.app.Application
import org.bouncycastle.asn1.ASN1Encodable
import org.bouncycastle.asn1.ASN1Integer
import org.bouncycastle.asn1.ASN1Set
import org.bouncycastle.asn1.DERSequence
import org.bouncycastle.asn1.pkcs.Attribute
import org.bouncycastle.asn1.pkcs.CertificationRequest
import org.bouncycastle.asn1.x509.Extension
import org.bouncycastle.asn1.x509.Extensions
import org.bouncycastle.asn1.x509.GeneralName
import org.bouncycastle.asn1.x509.GeneralNames
import org.json.JSONArray
import org.json.JSONObject
import org.junit.After
import org.junit.Assert.*
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder
import org.junit.runner.RunWith
import org.openxplay.direlay.publicname.FakeHttpServer.Request
import org.openxplay.direlay.publicname.FakeHttpServer.Response
import org.openxplay.direlay.tls.TlsStore
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import java.math.BigInteger
import java.security.KeyFactory
import java.security.MessageDigest
import java.security.Signature
import java.security.interfaces.ECPublicKey
import java.security.spec.ECPoint
import java.security.spec.ECPublicKeySpec
import java.security.spec.X509EncodedKeySpec
import java.util.Base64

/** A full RFC 8555 order against an in-process CA that checks every JWS, nonce and the dns-01 TXT digest. */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [28], manifest = Config.NONE, application = Application::class)
class AcmeClientTest {
    @get:Rule val temporary = TemporaryFolder()
    private val name = "myrelay.duckdns.org"
    private val config = PublicNameConfig(true, DnsProviderId.DUCKDNS, name, "a1b2c3d4-0000-0000-0000-000000000000", true)
    private val provider = FakeProvider()
    private lateinit var acme: FakeAcme
    private lateinit var store: PublicNameStore
    private lateinit var issuer: AcmeIssuer
    private val pauses = mutableListOf<Long>()
    private var orders = 0

    /** The provider's TXT record; the fake name servers and the fake CA both read it. */
    private class FakeProvider : DnsProvider {
        @Volatile var txt: String? = null
        val events = mutableListOf<String>()
        override val nameServers = listOf("ns1.test", "ns2.test")
        override fun updateAddress(ipv4: String) { events.add("address $ipv4") }
        override fun setChallenge(value: String): ChallengeRecord { txt = value; events.add("set"); return ChallengeRecord("zone", "7") }
        override fun clearChallenge(record: ChallengeRecord) { txt = null; events.add("clear ${record.id}") }
    }

    @Before fun start() {
        acme = FakeAcme(name) { provider.txt }
        store = PublicNameStore(temporary.newFolder()).also { it.save(config) }
        val lookup = TxtLookup { servers, queried ->
            assertEquals("_acme-challenge.$name", queried)
            servers.map { TxtResult.Answer(listOfNotNull(provider.txt)) }
        }
        issuer = AcmeIssuer(UrlConnectionHttp(UrlConnectionHttp.userAgent("test"), httpsOnly = false), store, lookup,
            directoryOf = { acme.directory })
    }

    @After fun stop() {
        acme.close()
        assertTrue(acme.errors.joinToString(), acme.errors.isEmpty())
    }

    private fun issue(replaces: String? = null) = issuer.issue(config, provider, replaces, Pause { pauses.add(it) }) { orders++ }
    private fun failure(block: () -> Unit): PublicNameFailure =
        try { block(); throw AssertionError("expected a failure") } catch (failure: PublicNameFailure) { failure }

    @Test fun fullOrderIssuesAMatchingCertificateAndClearsTheTxtRecord() {
        val issued = issue()
        val leaf = issued.chain.first()
        assertEquals(2, issued.chain.size)
        assertTrue(AcmeCrypto.covers(leaf, name))
        assertEquals((leaf.publicKey as ECPublicKey).w, (KeyFactory.getInstance("EC").generatePublic(
            X509EncodedKeySpec(leaf.publicKey.encoded)) as ECPublicKey).w)
        // The private key belongs to the leaf.
        val signature = Signature.getInstance("SHA256withECDSA").run { initSign(issued.key); update(byteArrayOf(1)); sign() }
        assertTrue(Signature.getInstance("SHA256withECDSA").run { initVerify(leaf.publicKey); update(byteArrayOf(1)); verify(signature) })
        // The certificate key is new, separate from the account key.
        assertNotEquals(store.accountKey()!!.public.encoded.toList(), leaf.publicKey.encoded.toList())
        assertEquals(listOf("set", "clear 7"), provider.events)
        assertEquals(listOf("newAccount jwk", "newOrder kid", "authz kid", "challenge kid", "authz kid", "authz kid", "order kid",
            "finalize kid", "order kid", "order kid", "certificate kid"), acme.log)
        assertEquals(1, orders)
        assertEquals("${acme.base}/acct/1", store.accountUrl(acme.directory))
        // The settling wait after propagation, and the CA's Retry-After (2 s, then 1 s) while polling.
        assertEquals(listOf(5_000L, 2_000L, 1_000L), pauses)
        assertTrue(TlsStore.pem(issued.chain).startsWith("-----BEGIN CERTIFICATE-----\n"))
    }

    @Test fun laterOrdersReuseTheAccountAndSendReplacesOnlyWhenTheCaOffersAri() {
        issue()
        acme.log.clear()
        issue(replaces = "aYhba4dGQEHhs3uEe6CuLN4ByNQ.AIdlQyE")
        assertEquals("no second registration", "newOrder kid", acme.log.first())
        assertEquals("aYhba4dGQEHhs3uEe6CuLN4ByNQ.AIdlQyE", acme.replaces)
        acme.ari = false
        issuer = AcmeIssuer(UrlConnectionHttp("test", httpsOnly = false), store,
            { servers, _ -> servers.map { TxtResult.Answer(listOfNotNull(provider.txt)) } }, directoryOf = { acme.directory })
        issue(replaces = "aYhba4dGQEHhs3uEe6CuLN4ByNQ.AIdlQyE")
        assertNull(acme.replaces)
    }

    @Test fun badNonceIsRetriedOnceWithTheNonceItCarried() {
        acme.badNonces = 1
        issue()
        assertEquals(1, acme.log.count { it == "badNonce" })
        acme.badNonces = 2
        val failure = failure { issue() }
        assertEquals(PublicNameError.ACME_REJECTED, failure.error)
        assertEquals(2, acme.log.count { it == "badNonce" } - 1)
    }

    @Test fun rateLimitedOrderReportsTheCasRetryAfterAndTouchesNoDnsRecord() {
        acme.orderRetryAfter = 3_600
        val failure = failure { issue() }
        assertEquals(PublicNameError.ACME_RATE_LIMITED, failure.error)
        assertEquals(3_600_000L, failure.retryAfterMillis)
        assertTrue(provider.events.isEmpty())
        assertEquals("the guard counts the order that was sent", 1, orders)
    }

    @Test fun unknownAccountIsRegisteredAgain() {
        issue()
        acme.accounts.clear()
        acme.log.clear()
        orders = 0
        issue()
        assertEquals(listOf("unknown account", "newAccount jwk", "newOrder kid"), acme.log.take(3))
        assertEquals(2, orders)
        assertEquals("${acme.base}/acct/2", store.accountUrl(acme.directory))
    }

    @Test fun failedChallengeClearsTheTxtRecordAndShowsTheCasDetail() {
        acme.wrongDigest = true
        val failure = failure { issue() }
        assertEquals(PublicNameError.ACME_REJECTED, failure.error)
        assertEquals("Incorrect TXT record found", failure.detail)
        assertEquals(listOf("set", "clear 7"), provider.events)
        assertNull(provider.txt)
    }

    @Test fun renewalInformationIsReadWhenOffered() {
        val window = checkNotNull(issuer.renewalWindow(config, "aYhba4dGQEHhs3uEe6CuLN4ByNQ.AIdlQyE"))
        assertEquals(java.time.Instant.parse("2026-11-01T00:00:00Z").toEpochMilli(), window.start)
        assertEquals(java.time.Instant.parse("2026-11-02T00:00:00Z").toEpochMilli(), window.end)
        assertEquals(21_600_000L, window.retryAfterMillis)
        assertEquals("ari aYhba4dGQEHhs3uEe6CuLN4ByNQ.AIdlQyE", acme.log.single())
        acme.ari = false
        assertNull(issuer.renewalWindow(config, "x.y"))
    }
}

/** Just enough of an ACME CA (RFC 8555, RFC 9773) to check the client end to end. */
private class FakeAcme(private val name: String, private val txt: () -> String?) : AutoCloseable {
    val ca = TestCa("Fake ACME CA")
    private val server = FakeHttpServer(::handle)
    val base = server.base
    val directory = "$base/directory"
    @Volatile var ari = true
    @Volatile var badNonces = 0
    @Volatile var orderRetryAfter: Long? = null
    @Volatile var wrongDigest = false
    @Volatile var replaces: String? = null
    val accounts = mutableMapOf<String, Pair<ECPublicKey, String>>()
    val log = mutableListOf<String>()
    private val issued = mutableSetOf<String>()
    private val used = mutableSetOf<String>()
    private var counter = 0
    private var accountCounter = 0
    private var token = ""
    private var authorization = "pending"
    private var authorizationPolls = 0
    private var order = "pending"
    private var orderPolls = 0
    private var pem: String? = null

    private fun b64(text: String): ByteArray = Base64.getUrlDecoder().decode(text)
    private fun b64(bytes: ByteArray): String = Base64.getUrlEncoder().withoutPadding().encodeToString(bytes)
    private fun nonce() = "nonce-${++counter}".also { issued.add(it) }
    private fun json(status: Int, body: JSONObject, headers: Map<String, String> = emptyMap()) =
        Response(status, body.toString(), mapOf("Replay-Nonce" to nonce(), "Content-Type" to "application/json") + headers)
    private fun problem(status: Int, type: String, detail: String, headers: Map<String, String> = emptyMap()) =
        Response(status, JSONObject().put("type", "urn:ietf:params:acme:error:$type").put("detail", detail).toString(),
            mapOf("Replay-Nonce" to nonce(), "Content-Type" to "application/problem+json") + headers)

    @Synchronized fun handle(request: Request): Response = when {
        request.method == "GET" && request.path == "/directory" -> Response(200, JSONObject()
            .put("newNonce", "$base/new-nonce").put("newAccount", "$base/new-account").put("newOrder", "$base/new-order")
            .also { if (ari) it.put("renewalInfo", "$base/renewal-info") }.toString())
        request.method == "HEAD" && request.path == "/new-nonce" -> Response(200, ByteArray(0), mapOf("Replay-Nonce" to nonce()))
        request.method == "GET" && request.path.startsWith("/renewal-info/") -> {
            log.add("ari ${request.path.substringAfterLast('/')}")
            Response(200, JSONObject().put("suggestedWindow", JSONObject().put("start", "2026-11-01T00:00:00Z")
                .put("end", "2026-11-02T00:00:00Z")).toString(), mapOf("Retry-After" to "21600"))
        }
        request.method == "POST" -> post(request)
        else -> Response(404, "")
    }

    private fun post(request: Request): Response {
        assertEquals("application/jose+json", request.header("Content-Type"))
        val jws = JSONObject(request.text)
        assertEquals(setOf("protected", "payload", "signature"), jws.keys().asSequence().toSet())
        val protectedPart = jws.getString("protected")
        val payloadPart = jws.getString("payload")
        val header = JSONObject(String(b64(protectedPart)))
        assertEquals("ES256", header.getString("alg"))
        assertEquals(base + request.path, header.getString("url"))
        val nonce = header.getString("nonce")
        assertTrue("every nonce comes from this server", nonce in issued)
        assertTrue("no nonce is used twice", used.add(nonce))
        if (badNonces > 0) { badNonces--; log.add("badNonce"); return problem(400, "badNonce", "stale nonce") }
        assertTrue("exactly one of jwk and kid", header.has("jwk") != header.has("kid"))
        val (key, thumbprint) = if (header.has("jwk")) {
            assertEquals("/new-account", request.path)
            jwkKey(header.getJSONObject("jwk"))
        } else {
            assertNotEquals("/new-account", request.path)
            accounts[header.getString("kid")] ?: run { log.add("unknown account"); return problem(400, "accountDoesNotExist", "no such account") }
        }
        // ES256 in JWS is raw r‖s, 64 bytes; the JVM verifies DER.
        val raw = b64(jws.getString("signature"))
        assertEquals(64, raw.size)
        val der = DERSequence(arrayOf<ASN1Encodable>(ASN1Integer(BigInteger(1, raw.copyOfRange(0, 32))),
            ASN1Integer(BigInteger(1, raw.copyOfRange(32, 64))))).encoded
        assertTrue("JWS signature", Signature.getInstance("SHA256withECDSA").run {
            initVerify(key); update("$protectedPart.$payloadPart".toByteArray(Charsets.US_ASCII)); verify(der)
        })
        val payload = String(b64(payloadPart))
        val by = if (header.has("jwk")) "jwk" else "kid"
        return when (request.path) {
            "/new-account" -> {
                log.add("newAccount $by")
                val body = JSONObject(payload)
                assertEquals("terms only, no contact", setOf("termsOfServiceAgreed"), body.keys().asSequence().toSet())
                assertTrue(body.getBoolean("termsOfServiceAgreed"))
                val existing = accounts.entries.firstOrNull { it.value.first == key }?.key
                val kid = existing ?: "$base/acct/${++accountCounter}".also { accounts[it] = key to thumbprint }
                json(if (existing != null) 200 else 201, JSONObject().put("status", "valid"), mapOf("Location" to kid))
            }
            "/new-order" -> {
                log.add("newOrder $by")
                orderRetryAfter?.let { return problem(429, "rateLimited", "too many new orders", mapOf("Retry-After" to it.toString())) }
                val body = JSONObject(payload)
                val identifiers = body.getJSONArray("identifiers")
                assertEquals(1, identifiers.length())
                assertEquals("dns", identifiers.getJSONObject(0).getString("type"))
                assertEquals(name, identifiers.getJSONObject(0).getString("value"))
                replaces = body.optString("replaces").ifEmpty { null }
                token = b64(ByteArray(32).also { java.security.SecureRandom().nextBytes(it) })
                authorization = "pending"; authorizationPolls = 0; order = "pending"; orderPolls = 0; pem = null
                json(201, orderJson(), mapOf("Location" to "$base/order/1"))
            }
            "/authz/1" -> {
                log.add("authz $by")
                assertEquals("POST-as-GET", "", payload)
                if (authorization == "processing") authorization = if (++authorizationPolls < 2) "processing" else "valid"
                val dns = JSONObject().put("type", "dns-01").put("url", "$base/chall/1").put("token", token)
                    .put("status", if (authorization == "invalid") "invalid" else "pending")
                if (authorization == "invalid") dns.put("error", JSONObject().put("type", "urn:ietf:params:acme:error:unauthorized")
                    .put("detail", "Incorrect TXT record found"))
                json(200, JSONObject().put("status", if (authorization == "processing") "pending" else authorization)
                    .put("identifier", JSONObject().put("type", "dns").put("value", name))
                    .put("challenges", JSONArray().put(JSONObject().put("type", "http-01").put("url", "$base/chall/0").put("token", "x"))
                        .put(dns)), mapOf("Retry-After" to "2"))
            }
            "/chall/1" -> {
                log.add("challenge $by")
                assertEquals("{}", payload)
                // The TXT digest, computed here independently of the client.
                val expected = b64(MessageDigest.getInstance("SHA-256").digest("$token.$thumbprint".toByteArray(Charsets.US_ASCII)))
                authorization = if (!wrongDigest && txt() == expected) "processing" else "invalid"
                json(200, JSONObject().put("type", "dns-01").put("status", "processing").put("token", token))
            }
            "/order/1" -> {
                log.add("order $by")
                assertEquals("", payload)
                if (order == "processing" && ++orderPolls >= 2) order = "valid"
                json(200, orderJson(), mapOf("Retry-After" to "1"))
            }
            "/order/1/finalize" -> {
                log.add("finalize $by")
                assertEquals("ready", currentOrder())
                val request = CertificationRequest.getInstance(b64(JSONObject(payload).getString("csr")))
                val info = request.certificationRequestInfo
                val csrKey = KeyFactory.getInstance("EC").generatePublic(X509EncodedKeySpec(info.subjectPublicKeyInfo.encoded))
                assertTrue("CSR signature", Signature.getInstance("SHA256withECDSA").run {
                    initVerify(csrKey); update(info.encoded); verify(request.signature.bytes)
                })
                val extensions = Extensions.getInstance((Attribute.getInstance(info.attributes.getObjectAt(0)).attrValues as ASN1Set).getObjectAt(0))
                assertEquals(listOf(GeneralName(GeneralName.dNSName, name)),
                    GeneralNames.getInstance(extensions.getExtensionParsedValue(Extension.subjectAlternativeName)).names.toList())
                assertNotEquals("the certificate key is not the account key", key, csrKey)
                pem = TlsStore.pem(listOf(ca.leaf(name, csrKey), ca.certificate))
                order = "processing"
                json(200, orderJson(), mapOf("Retry-After" to "1"))
            }
            "/cert/1" -> {
                log.add("certificate $by")
                assertEquals("application/pem-certificate-chain", request.header("Accept"))
                Response(200, checkNotNull(pem), mapOf("Replay-Nonce" to nonce(), "Content-Type" to "application/pem-certificate-chain"))
            }
            else -> problem(404, "malformed", "unknown resource")
        }
    }

    private fun currentOrder() = if (order == "pending" && authorization == "valid") "ready" else order

    private fun orderJson(): JSONObject = JSONObject().put("status", currentOrder())
        .put("identifiers", JSONArray().put(JSONObject().put("type", "dns").put("value", name)))
        .put("authorizations", JSONArray().put("$base/authz/1")).put("finalize", "$base/order/1/finalize")
        .also { if (order == "valid") it.put("certificate", "$base/cert/1") }

    /** The JWK as RFC 7638 orders it, the thumbprint over exactly those bytes, and the key it describes. */
    private fun jwkKey(jwk: JSONObject): Pair<ECPublicKey, String> {
        assertEquals("EC", jwk.getString("kty"))
        assertEquals("P-256", jwk.getString("crv"))
        val x = b64(jwk.getString("x"))
        val y = b64(jwk.getString("y"))
        assertEquals(32, x.size)
        assertEquals(32, y.size)
        val canonical = """{"crv":"P-256","kty":"EC","x":"${jwk.getString("x")}","y":"${jwk.getString("y")}"}"""
        val params = (TestCa.keyPair().public as ECPublicKey).params
        val key = KeyFactory.getInstance("EC").generatePublic(ECPublicKeySpec(ECPoint(BigInteger(1, x), BigInteger(1, y)), params)) as ECPublicKey
        return key to b64(MessageDigest.getInstance("SHA-256").digest(canonical.toByteArray(Charsets.US_ASCII)))
    }

    /** Assertion failures inside the server, which the client only sees as an HTTP error. */
    val errors: List<Throwable> get() = server.errors

    override fun close() = server.close()
}
