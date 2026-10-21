// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-20
package org.openxplay.direlay.publicname

import org.json.JSONArray
import org.json.JSONException
import org.json.JSONObject
import java.io.IOException

/** One provider account: the A record of the name and the TXT record at `_acme-challenge.<name>`. */
interface DnsProvider {
    /** The provider's authoritative name servers, queried directly for the propagation check. */
    val nameServers: List<String>
    fun updateAddress(ipv4: String)
    fun setChallenge(value: String): ChallengeRecord
    fun clearChallenge(record: ChallengeRecord)
}

/** What [DnsProvider.clearChallenge] needs to find the record again. */
data class ChallengeRecord(val zone: String? = null, val id: String? = null)

/** Base URLs; tests point them at an in-process server. */
data class ProviderEndpoints(
    val duckdns: String = "https://www.duckdns.org",
    val dynv6: String = "https://dynv6.com",
)

fun dnsProvider(config: PublicNameConfig, http: PublicNameHttp, endpoints: ProviderEndpoints = ProviderEndpoints(),
    clock: () -> Long = System::currentTimeMillis): DnsProvider = when (checkNotNull(config.provider)) {
    DnsProviderId.DUCKDNS -> DuckDns(config.name, config.token, http, endpoints.duckdns, clock)
    DnsProviderId.DYNV6 -> Dynv6(config.name, config.token, http, endpoints.dynv6, clock)
}

private const val CHALLENGE_LABEL = "_acme-challenge"

/** 2xx passes; 401/403 → token-rejected, 404 → name-not-found, 429 → provider-rate-limited, else unreachable. */
internal fun providerCall(http: PublicNameHttp, request: HttpRequest, clock: () -> Long): HttpResponse {
    val response = try { http.send(request) } catch (_: IOException) {
        throw PublicNameFailure(PublicNameError.PROVIDER_UNREACHABLE)
    }
    return when (response.status) {
        in 200..299 -> response
        401, 403 -> throw PublicNameFailure(PublicNameError.TOKEN_REJECTED)
        404 -> throw PublicNameFailure(PublicNameError.NAME_NOT_FOUND)
        429 -> throw PublicNameFailure(PublicNameError.PROVIDER_RATE_LIMITED, retryAfterMillis(response.header("Retry-After"), clock()))
        else -> throw PublicNameFailure(PublicNameError.PROVIDER_UNREACHABLE)
    }
}

private inline fun <T> parsed(block: () -> T): T = try { block() } catch (_: JSONException) {
    throw PublicNameFailure(PublicNameError.PROVIDER_UNREACHABLE)
}

/** Record and zone ids go into URL paths, so only plain identifiers pass. */
private fun identifier(value: Any): String = value.toString().takeIf { it.matches(Regex("[A-Za-z0-9_-]{1,64}")) }
    ?: throw PublicNameFailure(PublicNameError.PROVIDER_UNREACHABLE)

/** One TXT per name (shared by its sub-names), so challenges run one at a time. */
internal class DuckDns(name: String, private val token: String, private val http: PublicNameHttp,
    private val base: String, private val clock: () -> Long) : DnsProvider {
    private val label = name.removeSuffix(PublicNameRules.DUCKDNS_SUFFIX)
    override val nameServers = (1..9).map { "ns$it.duckdns.org" }

    private fun update(vararg parameters: Pair<String, String>) {
        val response = providerCall(http, HttpRequest("GET",
            "$base/update?" + query("domains" to label, "token" to token, *parameters)), clock)
        when (response.text.lineSequence().firstOrNull()?.trim()) {
            "OK" -> Unit
            "KO" -> throw PublicNameFailure(PublicNameError.TOKEN_REJECTED)
            else -> throw PublicNameFailure(PublicNameError.PROVIDER_UNREACHABLE)
        }
    }

    override fun updateAddress(ipv4: String) = update("ip" to ipv4)
    override fun setChallenge(value: String): ChallengeRecord { update("txt" to value); return ChallengeRecord() }
    override fun clearChallenge(record: ChallengeRecord) = update("txt" to "", "clear" to "true")
}

internal class Dynv6(private val name: String, private val token: String, private val http: PublicNameHttp,
    private val base: String, private val clock: () -> Long) : DnsProvider {
    private val bearer = mapOf("Authorization" to "Bearer $token")
    override val nameServers = listOf("ns1.dynv6.com", "ns2.dynv6.com", "ns3.dynv6.com")

    override fun updateAddress(ipv4: String) {
        providerCall(http, HttpRequest("GET", "$base/api/update?" + query("zone" to name, "token" to token, "ipv4" to ipv4)), clock)
    }

    override fun setChallenge(value: String): ChallengeRecord {
        val zone = providerCall(http, HttpRequest("GET", "$base/api/v2/zones/by-name/$name", bearer), clock)
            .let { response -> parsed { identifier(JSONObject(response.text).get("id")) } }
        val records = providerCall(http, HttpRequest("GET", "$base/api/v2/zones/$zone/records", bearer), clock)
            .let { response -> parsed { JSONArray(response.text) } }
        // A stale challenge from an interrupted run would otherwise be served alongside the new one.
        for (index in 0 until records.length()) {
            val record = records.optJSONObject(index) ?: continue
            if (record.optString("type") == "TXT" && record.optString("name") == CHALLENGE_LABEL) {
                providerCall(http, HttpRequest("DELETE", "$base/api/v2/zones/$zone/records/${identifier(record.get("id"))}", bearer), clock)
            }
        }
        val body = JSONObject().put("type", "TXT").put("name", CHALLENGE_LABEL).put("data", value)
        val created = providerCall(http, HttpRequest("POST", "$base/api/v2/zones/$zone/records",
            bearer + ("Content-Type" to "application/json"), body.toString().toByteArray()), clock)
        return ChallengeRecord(zone, parsed { identifier(JSONObject(created.text).get("id")) })
    }

    override fun clearChallenge(record: ChallengeRecord) {
        val zone = record.zone ?: return
        val id = record.id ?: return
        providerCall(http, HttpRequest("DELETE", "$base/api/v2/zones/$zone/records/$id", bearer), clock)
    }
}
