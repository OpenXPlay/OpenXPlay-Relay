// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-20
package org.openxplay.direlay.publicname

import android.util.Log
import org.json.JSONException
import org.json.JSONObject
import java.io.IOException
import java.security.KeyPair
import java.security.KeyPairGenerator
import java.security.PrivateKey
import java.security.cert.X509Certificate
import java.security.interfaces.ECPublicKey
import java.security.spec.ECGenParameterSpec
import java.time.OffsetDateTime

/** An RFC 8555 problem document; [type] without the `urn:ietf:params:acme:error:` prefix. */
internal class AcmeProblem(val type: String, val detail: String?, val status: Int, val retryAfterMillis: Long?) : Exception(type)

internal class AcmeResource(val url: String?, val json: JSONObject, val retryAfterMillis: Long?)

/** Minimal RFC 8555 client: ES256 JWS, nonces, POST-as-GET; [key] may be null for unauthenticated GETs only. */
internal class AcmeClient(private val http: PublicNameHttp, private val directoryUrl: String, private val key: KeyPair?,
    var accountUrl: String?, private val clock: () -> Long) {
    private var directory: JSONObject? = null
    private var nonce: String? = null

    fun directory(): JSONObject = directory ?: json(get(directoryUrl)).also { directory = it }
    private fun resource(name: String): String = directory().optString(name).takeIf { it.isNotEmpty() }
        ?: throw AcmeProblem("malformed", "directory has no $name", 0, null)
    val renewalInfo: String? get() = directory().optString("renewalInfo").takeIf { it.isNotEmpty() }

    /** `newAccount` with only the terms agreement: no contact address is sent. */
    fun register(): String {
        val response = post(resource("newAccount"), JSONObject().put("termsOfServiceAgreed", true).toString(), jwk = true)
        return response.header("Location")?.also { accountUrl = it }
            ?: throw AcmeProblem("malformed", "no account URL", response.status, null)
    }

    fun newOrder(name: String, replaces: String?): AcmeResource {
        val identifiers = org.json.JSONArray().put(JSONObject().put("type", "dns").put("value", name))
        val payload = JSONObject().put("identifiers", identifiers)
        replaces?.let { payload.put("replaces", it) }
        val response = post(resource("newOrder"), payload.toString())
        return AcmeResource(response.header("Location") ?: throw AcmeProblem("malformed", "no order URL", response.status, null),
            json(response), retryAfterMillis(response.header("Retry-After"), clock()))
    }

    fun fetch(url: String): AcmeResource = post(url, null).let { AcmeResource(url, json(it), retryAfterMillis(it.header("Retry-After"), clock())) }
    fun respond(challenge: String): JSONObject = json(post(challenge, "{}"))
    fun finalize(url: String, csr: ByteArray): JSONObject =
        json(post(url, JSONObject().put("csr", AcmeCrypto.base64Url(csr)).toString()))
    fun download(url: String): String = post(url, null, accept = "application/pem-certificate-chain").text

    /** RFC 9773 renewal information; null when the directory offers none. */
    fun renewalWindow(certId: String): RenewalWindow? {
        val base = renewalInfo ?: return null
        val response = get(base.trimEnd('/') + "/" + certId)
        return try {
            val window = json(response).getJSONObject("suggestedWindow")
            RenewalWindow(time(window.getString("start")), time(window.getString("end")),
                retryAfterMillis(response.header("Retry-After"), clock()))
        } catch (_: JSONException) { throw AcmeProblem("malformed", "renewal information", response.status, null) }
          catch (_: java.time.DateTimeException) { throw AcmeProblem("malformed", "renewal information", response.status, null) }
    }

    private fun time(text: String) = OffsetDateTime.parse(text).toInstant().toEpochMilli()

    private fun get(url: String): HttpResponse {
        val response = send(HttpRequest("GET", url))
        if (response.status !in 200..299) throw problem(response)
        return response
    }

    private fun newNonce(): String {
        val response = send(HttpRequest("HEAD", resource("newNonce")))
        if (response.status !in 200..299) throw problem(response)
        return response.header("Replay-Nonce") ?: throw AcmeProblem("badNonce", "no nonce", response.status, null)
    }

    /** POST-as-GET when [payload] is null; a `badNonce` rejection is retried once with the nonce it carried. */
    private fun post(url: String, payload: String?, jwk: Boolean = false, accept: String? = null): HttpResponse {
        val pair = checkNotNull(key) { "Signing key required" }
        var attempt = 0
        while (true) {
            val protected = JSONObject().put("alg", "ES256").put("nonce", nonce ?: newNonce()).put("url", url)
            nonce = null
            if (jwk) protected.put("jwk", JSONObject(AcmeCrypto.jwk(pair.public as ECPublicKey)))
            else protected.put("kid", checkNotNull(accountUrl) { "Account required" })
            val header = AcmeCrypto.base64Url(protected.toString())
            val body = payload?.let(AcmeCrypto::base64Url).orEmpty()
            val signature = AcmeCrypto.base64Url(AcmeCrypto.es256(pair.private, "$header.$body".toByteArray(Charsets.US_ASCII)))
            val jws = JSONObject().put("protected", header).put("payload", body).put("signature", signature)
            val headers = mapOf("Content-Type" to "application/jose+json") + (accept?.let { mapOf("Accept" to it) } ?: emptyMap())
            val response = send(HttpRequest("POST", url, headers, jws.toString().toByteArray(Charsets.UTF_8)))
            response.header("Replay-Nonce")?.let { nonce = it }
            if (response.status in 200..299) return response
            val problem = problem(response)
            if (problem.type != "badNonce" || attempt++ > 0) throw problem
        }
    }

    private fun send(request: HttpRequest): HttpResponse = try { http.send(request) } catch (_: IOException) {
        throw PublicNameFailure(PublicNameError.PROVIDER_UNREACHABLE)
    }

    private fun json(response: HttpResponse): JSONObject = try { JSONObject(response.text) } catch (_: JSONException) {
        throw AcmeProblem("malformed", "unreadable response", response.status, null)
    }

    private fun problem(response: HttpResponse): AcmeProblem {
        val json = runCatching { JSONObject(response.text) }.getOrNull()
        val type = json?.optString("type").orEmpty().removePrefix(ERROR_PREFIX)
            .ifEmpty { if (response.status == 429) "rateLimited" else "http-${response.status}" }
        return AcmeProblem(type, json?.optString("detail")?.takeIf { it.isNotEmpty() }, response.status,
            retryAfterMillis(response.header("Retry-After"), clock()))
    }

    companion object { const val ERROR_PREFIX = "urn:ietf:params:acme:error:" }
}

fun interface Pause { fun pause(millis: Long) }
class IssuedCertificate(val chain: List<X509Certificate>, val key: PrivateKey)
data class RenewalWindow(val start: Long, val end: Long, val retryAfterMillis: Long?)

/** Certificate requests and renewal information, behind an interface so the scheduler is tested without a CA. */
interface CertificateIssuer {
    /** [beforeOrder] runs before every `newOrder` request, so the local order guard counts each one. */
    fun issue(config: PublicNameConfig, provider: DnsProvider, replaces: String?, pause: Pause,
        beforeOrder: () -> Unit): IssuedCertificate
    /** Null when the CA offers no ARI. */
    fun renewalWindow(config: PublicNameConfig, certId: String): RenewalWindow?
}

/** One DNS-01 order: TXT through the provider, propagation checked at its name servers, TXT always cleared. */
class AcmeIssuer(private val http: PublicNameHttp, private val store: PublicNameStore, private val txt: TxtLookup,
    private val clock: () -> Long = System::currentTimeMillis,
    /** Tests point the environments at an in-process CA. */
    private val directoryOf: (AcmeEnvironment) -> String = { it.directory }) : CertificateIssuer {

    override fun issue(config: PublicNameConfig, provider: DnsProvider, replaces: String?, pause: Pause,
        beforeOrder: () -> Unit): IssuedCertificate = mapped {
        val directory = directoryOf(config.acme)
        val account = checkNotNull(store.accountKey()) { "Configuration removed" }
        val client = AcmeClient(http, directory, account, store.accountUrl(directory), clock)
        fun register() { store.saveAccountUrl(directory, client.register()) }
        fun newOrder(replacing: String?): AcmeResource { beforeOrder(); return client.newOrder(config.name, replacing) }
        if (client.accountUrl == null) register()
        val order = try {
            try { newOrder(replaces?.takeIf { client.renewalInfo != null }) }
            catch (problem: AcmeProblem) { if (problem.type != "alreadyReplaced" || replaces == null) throw problem; newOrder(null) }
        } catch (problem: AcmeProblem) {
            if (problem.type != "accountDoesNotExist") throw problem
            register()
            newOrder(null)
        }
        val orderUrl = checkNotNull(order.url)
        val authorizations = order.json.optJSONArray("authorizations") ?: throw rejected("no authorizations")
        for (index in 0 until authorizations.length()) authorize(client, authorizations.getString(index), config, provider, pause, account)
        var state = poll(client, orderUrl, pause) { it.optString("status") != "pending" }
        if (state.optString("status") == "ready") {
            val certificateKey = KeyPairGenerator.getInstance("EC").apply { initialize(ECGenParameterSpec("secp256r1")) }.generateKeyPair()
            state = client.finalize(state.getString("finalize"), AcmeCrypto.csr(config.name, certificateKey))
            if (state.optString("status") in setOf("ready", "processing")) {
                state = poll(client, orderUrl, pause) { it.optString("status") !in setOf("ready", "processing") }
            }
            if (state.optString("status") != "valid") throw rejected(state.optJSONObject("error")?.optString("detail")
                ?.takeIf { it.isNotEmpty() } ?: "order ${state.optString("status")}")
            val chain = AcmeCrypto.parseChain(client.download(state.getString("certificate")))
            val leaf = chain.firstOrNull() ?: throw rejected("empty certificate chain")
            if ((leaf.publicKey as? ECPublicKey)?.w != (certificateKey.public as ECPublicKey).w ||
                !AcmeCrypto.covers(leaf, config.name)) throw rejected("certificate does not match the request")
            leaf.checkValidity()
            return@mapped IssuedCertificate(chain, certificateKey.private)
        }
        throw rejected("order ${state.optString("status")}")
    }

    private fun authorize(client: AcmeClient, url: String, config: PublicNameConfig, provider: DnsProvider, pause: Pause, account: KeyPair) {
        val authorization = client.fetch(url).json
        when (authorization.optString("status")) {
            "valid" -> return
            "pending" -> Unit
            else -> throw rejected("authorization ${authorization.optString("status")}")
        }
        val challenges = authorization.optJSONArray("challenges") ?: throw rejected("no challenges")
        val challenge = (0 until challenges.length()).map { challenges.getJSONObject(it) }
            .firstOrNull { it.optString("type") == "dns-01" } ?: throw rejected("no dns-01 challenge")
        val value = AcmeCrypto.dnsValue(AcmeCrypto.keyAuthorization(challenge.getString("token"), account.public as ECPublicKey))
        val record = provider.setChallenge(value)
        try {
            TxtPropagation(txt, pause, clock).await(provider.nameServers, "_acme-challenge.${config.name}", value)
            client.respond(challenge.getString("url"))
            val done = poll(client, url, pause) { it.optString("status") != "pending" }
            if (done.optString("status") != "valid") {
                val failed = done.optJSONArray("challenges")?.let { all -> (0 until all.length()).map { all.getJSONObject(it) } }
                    ?.firstOrNull { it.optString("type") == "dns-01" }?.optJSONObject("error")
                throw rejected(failed?.optString("detail")?.takeIf { it.isNotEmpty() } ?: "authorization ${done.optString("status")}")
            }
        } finally {
            // A failure here must not hide the attempt's own outcome; the next set replaces a stale record.
            try { provider.clearChallenge(record) }
            catch (failure: PublicNameFailure) { Log.w(TAG, "TXT record not cleared: ${failure.error.key}") }
            catch (failure: RuntimeException) { Log.w(TAG, "TXT record not cleared: ${failure.javaClass.simpleName}") }
        }
    }

    /** Honours Retry-After (default 3 s) for at most 120 s. */
    private fun poll(client: AcmeClient, url: String, pause: Pause, done: (JSONObject) -> Boolean): JSONObject {
        val start = clock()
        while (true) {
            val resource = client.fetch(url)
            if (done(resource.json)) return resource.json
            if (resource.json.optString("status") == "invalid") return resource.json
            val wait = (resource.retryAfterMillis ?: 3_000).coerceIn(1_000, POLL_LIMIT_MS)
            if (clock() - start + wait > POLL_LIMIT_MS) throw rejected("no answer in time")
            pause.pause(wait)
        }
    }

    override fun renewalWindow(config: PublicNameConfig, certId: String): RenewalWindow? = mapped {
        AcmeClient(http, directoryOf(config.acme), null, null, clock).renewalWindow(certId)
    }

    private inline fun <T> mapped(block: () -> T): T = try { block() } catch (problem: AcmeProblem) {
        throw when {
            problem.type == "rateLimited" || problem.status == 429 ->
                PublicNameFailure(PublicNameError.ACME_RATE_LIMITED, problem.retryAfterMillis)
            else -> rejected(problem.detail ?: problem.type)
        }
    } catch (_: JSONException) { throw rejected("unreadable response") }
      catch (_: java.security.cert.CertificateException) { throw rejected("certificate not usable") }

    companion object {
        private const val TAG = "OpenXPlay Relay"
        private const val POLL_LIMIT_MS = 120_000L

        /** CA wording is shown to the user: one short printable line. */
        internal fun rejected(detail: String) = PublicNameFailure(PublicNameError.ACME_REJECTED,
            detail = detail.map { if (it < ' ' || it == '\u007f') ' ' else it }.joinToString("").trim().take(200))
    }
}
