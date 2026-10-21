// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-20
package org.openxplay.direlay.publicname

import org.json.JSONObject
import org.json.JSONTokener
import java.nio.ByteBuffer
import java.nio.charset.CodingErrorAction
import java.util.Locale

/** The supported dynamic DNS services; [key] is the configuration file value. */
enum class DnsProviderId(val key: String, val label: String) {
    DUCKDNS("duckdns", "DuckDNS"), DYNV6("dynv6", "dynv6");
    companion object { fun of(key: String): DnsProviderId? = entries.firstOrNull { it.key == key } }
}

enum class AcmeEnvironment(val key: String, val directory: String) {
    PRODUCTION("production", "https://acme-v02.api.letsencrypt.org/directory"),
    /** Let's Encrypt staging, for testing; its certificates are not trusted. */
    STAGING("staging", "https://acme-staging-v02.api.letsencrypt.org/directory");
    companion object { fun of(key: String): AcmeEnvironment? = entries.firstOrNull { it.key == key } }
}

/** A stored or imported configuration; it may be partly filled in. The token never appears in [toString]. */
data class PublicNameConfig(val enabled: Boolean, val provider: DnsProviderId?, val name: String, val token: String,
    val agreeTerms: Boolean, val acme: AcmeEnvironment = AcmeEnvironment.PRODUCTION) {
    /** Every field needed for an address update and a certificate request, terms included. */
    val complete: Boolean get() = provider != null && PublicNameRules.nameValid(provider, name) &&
        PublicNameRules.tokenValid(provider, token) && agreeTerms
    override fun toString() = "PublicNameConfig(enabled=$enabled, provider=${provider?.key}, name=$name, " +
        "token=${if (token.isEmpty()) "none" else "set"}, agreeTerms=$agreeTerms, acme=${acme.key})"
}

object PublicNameRules {
    private val label = Regex("[a-z0-9](?:[a-z0-9-]{0,61}[a-z0-9])?")
    private val uuid = Regex("[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}")
    const val DUCKDNS_SUFFIX = ".duckdns.org"

    fun normalizeName(raw: String): String = raw.lowercase(Locale.ROOT)

    /** LDH labels, at most 253 characters, no trailing dot; DuckDNS names are exactly `<label>.duckdns.org`. */
    fun nameValid(provider: DnsProviderId?, name: String): Boolean {
        if (name.isEmpty() || name.length > 253 || name.endsWith('.')) return false
        val labels = name.split('.')
        if (labels.size < 2 || labels.any { !label.matches(it) }) return false
        return provider != DnsProviderId.DUCKDNS || labels.size == 3 && name.endsWith(DUCKDNS_SUFFIX)
    }

    /** 1–256 printable ASCII characters without spaces; DuckDNS tokens are UUIDs. */
    fun tokenValid(provider: DnsProviderId?, token: String): Boolean =
        token.length in 1..256 && token.all { it in '!'..'~' } && (provider != DnsProviderId.DUCKDNS || uuid.matches(token))
}

/** Stable per-key error codes shared with the Linux relay and the website. */
enum class ConfigProblem(val code: String) {
    MISSING("missing"), INVALID("invalid"), UNKNOWN_KEY("unknown-key"), BAD_VERSION("bad-version"),
    TOO_LARGE("too-large"), NOT_JSON("not-json"),
}

/** [key] is the configuration key, or [FILE] for problems with the whole file. */
data class ConfigError(val key: String, val problem: ConfigProblem) {
    companion object { const val FILE = "file" }
}

sealed class ConfigParse {
    data class Valid(val config: PublicNameConfig) : ConfigParse()
    data class Invalid(val errors: List<ConfigError>) : ConfigParse()
}

/** The configuration file format (`public-name.json` on Linux, the import file on Android). */
object PublicNameFile {
    const val MAX_BYTES = 16 * 1024
    const val VERSION = "version"
    const val ENABLED = "enabled"
    const val PROVIDER = "provider"
    const val NAME = "name"
    const val TOKEN = "token"
    const val TERMS = "agreeLetsEncryptTerms"
    const val ACME = "acme"
    private val keys = listOf(VERSION, ENABLED, PROVIDER, NAME, TOKEN, TERMS, ACME)

    /**
     * An imported file must be complete apart from the terms; [partial] accepts the stored form,
     * which the settings screen may save while provider, name or token are still empty.
     */
    fun parse(bytes: ByteArray, partial: Boolean = false): ConfigParse {
        if (bytes.size > MAX_BYTES) return invalid(ConfigError.FILE, ConfigProblem.TOO_LARGE)
        val text = try {
            Charsets.UTF_8.newDecoder().onMalformedInput(CodingErrorAction.REPORT)
                .onUnmappableCharacter(CodingErrorAction.REPORT).decode(ByteBuffer.wrap(bytes)).toString()
        } catch (_: java.nio.charset.CharacterCodingException) { return invalid(ConfigError.FILE, ConfigProblem.NOT_JSON) }
        val json = try {
            val tokener = JSONTokener(text)
            (tokener.nextValue() as? JSONObject)?.takeIf { tokener.nextClean() == '\u0000' }
        } catch (_: org.json.JSONException) { null } ?: return invalid(ConfigError.FILE, ConfigProblem.NOT_JSON)
        val errors = mutableListOf<ConfigError>()
        json.keys().asSequence().filter { it !in keys }.sorted().forEach { errors.add(ConfigError(it, ConfigProblem.UNKNOWN_KEY)) }
        fun missing(key: String) = errors.add(ConfigError(key, ConfigProblem.MISSING))
        fun invalid(key: String) = errors.add(ConfigError(key, ConfigProblem.INVALID))
        when (val version = json.opt(VERSION)) {
            null -> missing(VERSION)
            !is Number -> errors.add(ConfigError(VERSION, ConfigProblem.BAD_VERSION))
            else -> if (version.toDouble() != 1.0) errors.add(ConfigError(VERSION, ConfigProblem.BAD_VERSION))
        }
        fun bool(key: String, default: Boolean): Boolean = when (val value = json.opt(key)) {
            null -> default
            is Boolean -> value
            else -> { invalid(key); default }
        }
        fun string(key: String): String? = when (val value = json.opt(key)) {
            null -> { if (!partial) missing(key); null }
            is String -> value
            else -> { invalid(key); null }
        }
        val enabled = bool(ENABLED, true)
        val provider = string(PROVIDER)?.let { key ->
            DnsProviderId.of(key).also { if (it == null && !(partial && key.isEmpty())) invalid(PROVIDER) }
        }
        val name = string(NAME)?.let(PublicNameRules::normalizeName)
        if (name != null && !(partial && name.isEmpty()) && !PublicNameRules.nameValid(provider, name)) invalid(NAME)
        val token = string(TOKEN)
        if (token != null && !(partial && token.isEmpty()) && !PublicNameRules.tokenValid(provider, token)) invalid(TOKEN)
        val terms = bool(TERMS, false)
        val acme = when (val value = json.opt(ACME)) {
            null -> AcmeEnvironment.PRODUCTION
            is String -> AcmeEnvironment.of(value) ?: run { invalid(ACME); AcmeEnvironment.PRODUCTION }
            else -> { invalid(ACME); AcmeEnvironment.PRODUCTION }
        }
        if (errors.isNotEmpty()) return ConfigParse.Invalid(errors.sortedBy { keys.indexOf(it.key).let { i -> if (i < 0) keys.size else i } })
        return ConfigParse.Valid(PublicNameConfig(enabled, provider, name.orEmpty(), token.orEmpty(), terms, acme))
    }

    /** The stored form, token included; only ever written to the private store. */
    fun encode(config: PublicNameConfig): ByteArray = JSONObject()
        .put(VERSION, 1).put(ENABLED, config.enabled).put(PROVIDER, config.provider?.key.orEmpty())
        .put(NAME, config.name).put(TOKEN, config.token).put(TERMS, config.agreeTerms).put(ACME, config.acme.key)
        .toString().toByteArray(Charsets.UTF_8)

    private fun invalid(key: String, problem: ConfigProblem) = ConfigParse.Invalid(listOf(ConfigError(key, problem)))
}
