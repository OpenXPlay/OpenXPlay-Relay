// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-20
package org.openxplay.direlay.publicname

import org.bouncycastle.asn1.ASN1Encodable
import org.bouncycastle.asn1.ASN1Integer
import org.bouncycastle.asn1.DERBitString
import org.bouncycastle.asn1.DERSequence
import org.bouncycastle.asn1.x500.X500Name
import org.bouncycastle.asn1.x509.AlgorithmIdentifier
import org.bouncycastle.asn1.x509.AuthorityKeyIdentifier
import org.bouncycastle.asn1.x509.BasicConstraints
import org.bouncycastle.asn1.x509.Extension
import org.bouncycastle.asn1.x509.Extensions
import org.bouncycastle.asn1.x509.GeneralName
import org.bouncycastle.asn1.x509.GeneralNames
import org.bouncycastle.asn1.x509.SubjectKeyIdentifier
import org.bouncycastle.asn1.x509.SubjectPublicKeyInfo
import org.bouncycastle.asn1.x509.Time
import org.bouncycastle.asn1.x509.V3TBSCertificateGenerator
import org.bouncycastle.asn1.x9.X9ObjectIdentifiers
import java.math.BigInteger
import java.security.KeyPair
import java.security.KeyPairGenerator
import java.security.MessageDigest
import java.security.PublicKey
import java.security.SecureRandom
import java.security.Signature
import java.security.cert.CertificateFactory
import java.security.cert.X509Certificate
import java.security.spec.ECGenParameterSpec
import java.util.Date

/** A throwaway test CA (bcprov ASN.1 only, like the relay's own certificates); nothing here is trusted anywhere. */
internal class TestCa(val name: String = "OpenXPlay test CA") {
    val pair: KeyPair = keyPair()
    val ownKeyId: ByteArray = keyId(pair.public)
    val certificate: X509Certificate = build(X500Name("CN=$name"), pair.public, null, ownKeyId,
        System.currentTimeMillis() - DAY, System.currentTimeMillis() + 3650 * DAY, BigInteger.ONE, ca = true)

    /** A leaf for [dnsName], signed by this CA, with an authority key identifier for RFC 9773 certIDs. */
    fun leaf(dnsName: String, key: PublicKey, notBefore: Long = System.currentTimeMillis() - 60_000,
        notAfter: Long = notBefore + 90 * DAY, serial: BigInteger = BigInteger(64, SecureRandom()).add(BigInteger.ONE),
        authorityKeyId: ByteArray = ownKeyId): X509Certificate =
        build(X500Name("CN=$dnsName"), key, dnsName, null, notBefore, notAfter, serial, ca = false, authorityKeyId)

    private fun build(subject: X500Name, key: PublicKey, dnsName: String?, subjectKeyId: ByteArray?, notBefore: Long, notAfter: Long,
        serial: BigInteger, ca: Boolean, authorityKeyId: ByteArray? = null): X509Certificate {
        val algorithm = AlgorithmIdentifier(X9ObjectIdentifiers.ecdsa_with_SHA256)
        val extensions = buildList {
            add(Extension.create(Extension.basicConstraints, true, BasicConstraints(ca)))
            if (dnsName != null) add(Extension.create(Extension.subjectAlternativeName, false, GeneralNames(GeneralName(GeneralName.dNSName, dnsName))))
            if (subjectKeyId != null) add(Extension.create(Extension.subjectKeyIdentifier, false, SubjectKeyIdentifier(subjectKeyId)))
            if (authorityKeyId != null) add(Extension.create(Extension.authorityKeyIdentifier, false, AuthorityKeyIdentifier(authorityKeyId)))
        }
        val tbs = V3TBSCertificateGenerator().apply {
            setSerialNumber(ASN1Integer(serial))
            setSignature(algorithm)
            setIssuer(X500Name("CN=$name"))
            setSubject(subject)
            setStartDate(Time(Date(notBefore)))
            setEndDate(Time(Date(notAfter)))
            setSubjectPublicKeyInfo(SubjectPublicKeyInfo.getInstance(key.encoded))
            setExtensions(Extensions(extensions.toTypedArray()))
        }.generateTBSCertificate()
        val signature = Signature.getInstance("SHA256withECDSA").run { initSign(pair.private); update(tbs.encoded); sign() }
        return CertificateFactory.getInstance("X.509").generateCertificate(
            DERSequence(arrayOf<ASN1Encodable>(tbs, algorithm, DERBitString(signature))).encoded.inputStream()) as X509Certificate
    }

    companion object {
        const val DAY = 86_400_000L
        fun keyPair(): KeyPair = KeyPairGenerator.getInstance("EC").apply { initialize(ECGenParameterSpec("secp256r1")) }.generateKeyPair()
        fun keyId(key: PublicKey): ByteArray = MessageDigest.getInstance("SHA-1").digest(SubjectPublicKeyInfo.getInstance(key.encoded).publicKeyData.bytes)
    }
}
