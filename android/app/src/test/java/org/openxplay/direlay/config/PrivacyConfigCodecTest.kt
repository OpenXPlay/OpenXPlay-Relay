// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-22
package org.openxplay.direlay.config

import android.app.Application
import org.bouncycastle.asn1.ASN1Encodable
import org.bouncycastle.asn1.ASN1Integer
import org.bouncycastle.asn1.DERBitString
import org.bouncycastle.asn1.DERSequence
import org.bouncycastle.asn1.x500.X500Name
import org.bouncycastle.asn1.x509.*
import org.bouncycastle.asn1.x9.X9ObjectIdentifiers
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.openxplay.direlay.publicname.*
import java.math.BigInteger
import java.security.*
import java.security.cert.CertificateFactory
import java.security.cert.X509Certificate
import java.security.spec.ECGenParameterSpec
import java.util.Date

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [28], manifest = Config.NONE, application = Application::class)
class PrivacyConfigCodecTest {
    private val meta = ConfigMeta("test", "2026-10-22T00:00:00Z")
    private val config = PublicNameConfig(true, DnsProviderId.DUCKDNS, "test.duckdns.org",
        "12345678-1234-1234-1234-123456789abc", true)
    private fun pair() = KeyPairGenerator.getInstance("EC").apply { initialize(ECGenParameterSpec("secp256r1")) }.generateKeyPair()
    private fun certificate(pair: KeyPair, expired: Boolean = false): ConfigCertificate {
        val algorithm = AlgorithmIdentifier(X9ObjectIdentifiers.ecdsa_with_SHA256)
        val subject = X500Name("CN=configuration test")
        val tbs = V3TBSCertificateGenerator().apply {
            setSerialNumber(ASN1Integer(BigInteger.ONE)); setSignature(algorithm)
            setIssuer(subject); setSubject(subject); setStartDate(Time(Date(0)))
            setEndDate(Time(Date(if (expired) 1 else 4102444800000L)))
            setSubjectPublicKeyInfo(SubjectPublicKeyInfo.getInstance(pair.public.encoded))
            setExtensions(Extensions(Extension.create(Extension.subjectAlternativeName, false,
                GeneralNames(GeneralName(GeneralName.dNSName, config.name)))))
        }.generateTBSCertificate()
        val signature = Signature.getInstance("SHA256withECDSA").run { initSign(pair.private); update(tbs.encoded); sign() }
        val leaf = CertificateFactory.getInstance("X.509").generateCertificate(
            DERSequence(arrayOf<ASN1Encodable>(tbs, algorithm, DERBitString(signature))).encoded.inputStream()) as X509Certificate
        return ConfigCertificate(listOf(leaf), pair.private, config.name)
    }
    @Test fun roundTripPreservesPublicNameSerializationAccountAndCertificates() {
        val material = certificate(pair())
        val account = ConfigAccount(pair(), config.acme.directory, "https://acme.example.org/account/1")
        val value = PrivacyConfig(config, material, account, mapOf(DnsProviderId.DUCKDNS to PublicNameProviderSetup(config.name, config.token)),
            material, "remote-credential")
        val text = PrivacyConfigCodec.encode(value, meta)
        assertTrue(JSONObject(text).getJSONObject("publicName").toString().toByteArray().contentEquals(PublicNameFile.encode(config)))
        val result = PrivacyConfigCodec.decode(text)
        assertTrue(result.errors.toString(), result.errors.isEmpty())
        val decoded = checkNotNull(result.value)
        assertTrue(decoded.publicName == config)
        assertTrue(decoded.providers == value.providers)
        assertTrue(decoded.mfiRemoteToken == value.mfiRemoteToken)
        assertTrue(decoded.publicNameAccount!!.pair.private.encoded.contentEquals(account.pair.private.encoded))
        assertTrue(decoded.publicNameAccount.pair.public.encoded.contentEquals(account.pair.public.encoded))
        assertTrue(decoded.publicNameCertificate!!.chain.first().encoded.contentEquals(material.chain.first().encoded))
        assertTrue(decoded.importedCertificate!!.key.encoded.contentEquals(material.key.encoded))
        for (description in listOf(value.toString(), material.toString(), account.toString(), result.toString())) {
            assertFalse(description.contains(config.token)); assertFalse(description.contains("BEGIN")); assertFalse(description.contains("remote-credential"))
        }
    }
    @Test fun rejectsMismatchedNameKeyExpiredAndMalformedPem() {
        val material = certificate(pair())
        fun json(certificate: ConfigCertificate = material) = JSONObject(PrivacyConfigCodec.encode(PrivacyConfig(config, certificate), meta))
        val wrongName = json().also { it.getJSONObject("publicNameCertificate").put("name", "other.duckdns.org") }
        val wrongKey = json().also { it.getJSONObject("publicNameCertificate").put("keyPem", PrivacyConfigCodec.keyPem(pair().private)) }
        val wrongSan = json().also { it.getJSONObject("publicName").put("name", "other.duckdns.org"); it.getJSONObject("publicNameCertificate").put("name", "other.duckdns.org") }
        val expired = json(certificate(pair(), true))
        val malformed = json().also { it.getJSONObject("publicNameCertificate").put("chainPem", "invalid") }
        for (value in listOf(wrongName, wrongKey, wrongSan, expired, malformed)) {
            val result = PrivacyConfigCodec.decode(value.toString())
            assertNull(result.value)
            assertTrue(result.errors.containsKey("publicNameCertificate"))
        }
    }
    @Test fun sectionsAreOptionalAndPublicNameUsesStrictParser() {
        val empty = PrivacyConfigCodec.decode(PrivacyConfigCodec.encode(PrivacyConfig(), meta))
        assertTrue(empty.errors.isEmpty()); assertTrue(empty.value!!.empty)
        val tokenOnly = PrivacyConfigCodec.decode(PrivacyConfigCodec.encode(PrivacyConfig(mfiRemoteToken = "credential"), meta))
        assertTrue(tokenOnly.errors.isEmpty()); assertNull(tokenOnly.value!!.publicName)
        val material = certificate(pair())
        val certificateOnly = PrivacyConfigCodec.encode(PrivacyConfig(publicNameCertificate = material), meta)
        assertTrue(PrivacyConfigCodec.decode(certificateOnly, config).errors.isEmpty())
        assertTrue(PrivacyConfigCodec.decode(certificateOnly).errors.isNotEmpty())
        val invalid = JSONObject(PrivacyConfigCodec.encode(PrivacyConfig(config), meta))
        invalid.getJSONObject("publicName").put("token", "invalid")
        assertTrue(PrivacyConfigCodec.decode(invalid.toString()).errors.containsKey("publicName.token"))
        invalid.getJSONObject("publicName").put("token", config.token).put("unknown", true)
        assertTrue(PrivacyConfigCodec.decode(invalid.toString()).errors.containsKey("publicName.unknown"))
    }
}
