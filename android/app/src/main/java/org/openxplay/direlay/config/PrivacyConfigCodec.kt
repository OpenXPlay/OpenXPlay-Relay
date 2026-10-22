// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-22
package org.openxplay.direlay.config

import org.json.JSONObject
import org.openxplay.direlay.publicname.*
import java.security.*
import java.security.cert.CertificateFactory
import java.security.cert.X509Certificate
import java.security.interfaces.ECPrivateKey
import java.security.spec.*
import java.util.Base64

class ConfigCertificate(val chain: List<X509Certificate>, val key: PrivateKey, val name: String? = null) {
    override fun toString() = "ConfigCertificate(redacted)"
}
class ConfigAccount(val pair: KeyPair, val directory: String, val url: String?) {
    override fun toString() = "ConfigAccount(redacted)"
}
class PrivacyConfig(val publicName: PublicNameConfig? = null, val publicNameCertificate: ConfigCertificate? = null,
    val publicNameAccount: ConfigAccount? = null, val providers: Map<DnsProviderId, PublicNameProviderSetup>? = null,
    val importedCertificate: ConfigCertificate? = null, val mfiRemoteToken: String? = null) {
    val empty: Boolean get() = publicName == null && publicNameCertificate == null && publicNameAccount == null &&
        providers.isNullOrEmpty() && importedCertificate == null && mfiRemoteToken.isNullOrEmpty()
    override fun toString() = "PrivacyConfig(redacted)"
}
class PrivacyConfigImport(val value: PrivacyConfig?, val errors: Map<String, String>) {
    override fun toString() = "PrivacyConfigImport(errors=${errors.size})"
}

/** Optional sections replace only their corresponding stores. Errors contain keys and codes only. */
object PrivacyConfigCodec {
    const val FORMAT = "openxplay-privacy-config"
    fun encode(value: PrivacyConfig, meta: ConfigMeta): String {
        val json = ConfigEnvelope.create(FORMAT, meta)
        value.publicName?.let { json.put("publicName", JSONObject(String(PublicNameFile.encode(it), Charsets.UTF_8))) }
        fun certificate(key: String, material: ConfigCertificate?) { material?.let {
            val section = JSONObject().put("chainPem", chainPem(it.chain)).put("keyPem", keyPem(it.key))
            if (key == "publicNameCertificate") section.put("name", it.name)
            json.put(key, section)
        } }
        certificate("publicNameCertificate", value.publicNameCertificate)
        certificate("importedCertificate", value.importedCertificate)
        value.publicNameAccount?.let {
            json.put("publicNameAccount", JSONObject().put("keyPem", keyPem(it.pair.private)).put("directory", it.directory).put("url", it.url ?: JSONObject.NULL))
        }
        value.providers?.let { providers ->
            json.put("providers", JSONObject().also { o -> providers.forEach { (id, setup) ->
                o.put(id.key, JSONObject().put("name", setup.name).put("token", setup.token))
            } })
        }
        value.mfiRemoteToken?.let { json.put("mfiRemoteToken", it) }
        return ConfigEnvelope.finish(json)
    }
    fun decode(bytes: ByteArray, currentPublicName: PublicNameConfig? = null): PrivacyConfigImport = try {
        decode(ConfigEnvelope.text(bytes), currentPublicName)
    } catch (e: ConfigFailure) { PrivacyConfigImport(null, mapOf(e.key to e.code)) }

    fun decode(text: String, currentPublicName: PublicNameConfig? = null): PrivacyConfigImport {
        val json = try { ConfigEnvelope.read(text, FORMAT) }
        catch (e: ConfigFailure) { return PrivacyConfigImport(null, mapOf(e.key to e.code)) }
        val errors = linkedMapOf<String, String>()
        fun <T> section(key: String, parse: (JSONObject) -> T): T? {
            if (!json.has(key)) return null
            return try { parse(json.getJSONObject(key)) } catch (_: Exception) { errors.putIfAbsent(key, "invalid"); null }
        }
        val config = section("publicName") {
            when (val parsed = PublicNameFile.parse(it.toString().toByteArray(Charsets.UTF_8))) {
                is ConfigParse.Valid -> parsed.config
                is ConfigParse.Invalid -> {
                    parsed.errors.forEach { error -> errors.putIfAbsent("publicName.${error.key}", error.problem.code) }
                    throw ConfigFailure("publicName", "invalid")
                }
            }
        }
        val publicCertificate = section("publicNameCertificate") {
            val name = it.string("name")
            require(name == (config ?: currentPublicName)?.name)
            certificate(it.string("chainPem"), it.string("keyPem"), name).also { material -> require(material.key.algorithm == "EC") }
        }
        val imported = section("importedCertificate") { certificate(it.string("chainPem"), it.string("keyPem")) }
        val account = section("publicNameAccount") {
            val pair = accountPair(privateKey(it.string("keyPem")))
            val directory = it.string("directory"); val url = it.nullableString("url")
            requireHttps(directory); url?.let(::requireHttps)
            ConfigAccount(pair, directory, url)
        }
        val providers = section("providers") { o -> o.keys().asSequence().associate { key ->
            val provider = requireNotNull(DnsProviderId.of(key))
            val entry = o.getJSONObject(key)
            val name = entry.string("name"); val token = entry.string("token")
            require(name.isEmpty() || PublicNameRules.nameValid(provider, name))
            require(token.isEmpty() || PublicNameRules.tokenValid(provider, token))
            provider to PublicNameProviderSetup(name, token)
        } }
        val token = if (json.has("mfiRemoteToken")) try { json.string("mfiRemoteToken").also { require('\u0000' !in it) } }
            catch (_: Exception) { errors["mfiRemoteToken"] = "invalid"; null } else null
        return PrivacyConfigImport(if (errors.isEmpty()) PrivacyConfig(config, publicCertificate, account, providers, imported, token) else null, errors)
    }
    private fun requireHttps(value: String) { java.net.URI(value).also { require(it.scheme == "https" && !it.host.isNullOrEmpty() && it.userInfo == null) } }
    private fun chainPem(chain: List<X509Certificate>): String = chain.joinToString("") {
        "-----BEGIN CERTIFICATE-----\n" + Base64.getMimeEncoder(64, "\n".toByteArray()).encodeToString(it.encoded) +
            "\n-----END CERTIFICATE-----\n"
    }
    fun keyPem(key: PrivateKey): String = "-----BEGIN PRIVATE KEY-----\n" +
        Base64.getMimeEncoder(64, "\n".toByteArray()).encodeToString(key.encoded) + "\n-----END PRIVATE KEY-----\n"

    private fun pem(text: String, label: String): List<ByteArray> {
        val pattern = Regex("-----BEGIN $label-----\\s*([A-Za-z0-9+/=\\r\\n]+)\\s*-----END $label-----")
        val matches = pattern.findAll(text).toList()
        require(matches.isNotEmpty() && pattern.replace(text, "").isBlank())
        return matches.map { Base64.getDecoder().decode(it.groupValues[1].filterNot(Char::isWhitespace)) }
    }
    fun privateKey(text: String): PrivateKey {
        val bytes = pem(text, "PRIVATE KEY").single()
        try {
            for (algorithm in listOf("EC", "RSA")) try {
                return KeyFactory.getInstance(algorithm).generatePrivate(PKCS8EncodedKeySpec(bytes))
            } catch (_: Exception) { /* Try the other supported key family. */ }
            throw IllegalArgumentException("Invalid private key")
        } finally { bytes.fill(0) }
    }
    fun certificate(chainPem: String, keyPem: String, name: String? = null): ConfigCertificate {
        try {
            val factory = CertificateFactory.getInstance("X.509")
            val chain = pem(chainPem, "CERTIFICATE").map { factory.generateCertificate(it.inputStream()) as X509Certificate }
            val key = privateKey(keyPem)
            val leaf = chain.first()
            leaf.checkValidity()
            if (name != null) require(leaf.subjectAlternativeNames.orEmpty().any { it.getOrNull(0) == 2 && (it.getOrNull(1) as? String).equals(name, true) })
            verifyKey(key, leaf.publicKey)
            return ConfigCertificate(chain, key, name)
        } catch (_: Exception) { throw IllegalArgumentException("Invalid certificate material") }
    }
    private fun verifyKey(key: PrivateKey, publicKey: PublicKey) {
        val algorithm = when (key.algorithm) { "EC" -> "SHA256withECDSA"; "RSA" -> "SHA256withRSA"; else -> error("key") }
        val probe = ByteArray(32).also { SecureRandom().nextBytes(it) }
        val signed = Signature.getInstance(algorithm).run { initSign(key); update(probe); sign() }
        require(Signature.getInstance(algorithm).run { initVerify(publicKey); update(probe); verify(signed) })
    }
    /** Account files contain PKCS#8 only; reconstruct the P-256 public point for the store. */
    private fun accountPair(key: PrivateKey): KeyPair {
        require(key is ECPrivateKey)
        val curve = org.bouncycastle.asn1.sec.SECNamedCurves.getByName("secp256r1")
        val point = curve.g.multiply(key.s).normalize()
        val parameters = AlgorithmParameters.getInstance("EC").apply { init(ECGenParameterSpec("secp256r1")) }.getParameterSpec(ECParameterSpec::class.java)
        require(key.params.order == parameters.order)
        val publicKey = KeyFactory.getInstance("EC").generatePublic(ECPublicKeySpec(ECPoint(point.affineXCoord.toBigInteger(), point.affineYCoord.toBigInteger()), parameters))
        verifyKey(key, publicKey)
        return KeyPair(publicKey, key)
    }
}
