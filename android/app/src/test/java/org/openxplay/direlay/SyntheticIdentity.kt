// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-14
package org.openxplay.direlay

import java.math.BigInteger
import java.security.KeyPairGenerator
import java.security.Signature
import java.security.cert.CertificateFactory
import java.security.spec.ECGenParameterSpec
import java.util.Date
import org.bouncycastle.asn1.ASN1Encodable
import org.bouncycastle.asn1.ASN1Integer
import org.bouncycastle.asn1.DERBitString
import org.bouncycastle.asn1.DERSequence
import org.bouncycastle.asn1.x500.X500Name
import org.bouncycastle.asn1.x509.AlgorithmIdentifier
import org.bouncycastle.asn1.x509.SubjectPublicKeyInfo
import org.bouncycastle.asn1.x509.Time
import org.bouncycastle.asn1.x509.V3TBSCertificateGenerator
import org.bouncycastle.asn1.x9.X9ObjectIdentifiers

/** Fresh self-signed test material; no accessory credentials are bundled. */
internal class SyntheticIdentity {
    val pair = KeyPairGenerator.getInstance("EC").apply {
        initialize(ECGenParameterSpec("secp256r1"))
    }.generateKeyPair()
    private val algorithm = AlgorithmIdentifier(X9ObjectIdentifiers.ecdsa_with_SHA256)
    private val name = X500Name("CN=DiRelay synthetic test only")
    private val tbs = V3TBSCertificateGenerator().apply {
        setSerialNumber(ASN1Integer(BigInteger.ONE))
        setSignature(algorithm)
        setIssuer(name)
        setSubject(name)
        setStartDate(Time(Date(0)))
        setEndDate(Time(Date(4102444800000L)))
        setSubjectPublicKeyInfo(SubjectPublicKeyInfo.getInstance(pair.public.encoded))
    }.generateTBSCertificate()
    private val signature = Signature.getInstance("SHA256withECDSA").run {
        initSign(pair.private); update(tbs.encoded); sign()
    }
    val certificate = CertificateFactory.getInstance("X.509").generateCertificate(
        DERSequence(arrayOf<ASN1Encodable>(tbs, algorithm, DERBitString(signature))).encoded.inputStream(),
    )
    val pkcs7: ByteArray = CertificateFactory.getInstance("X.509")
        .generateCertPath(listOf(certificate)).getEncoded("PKCS7")

    fun pkcs12(password: CharArray): ByteArray {
        val store = java.security.KeyStore.getInstance("PKCS12")
        store.load(null, password)
        store.setKeyEntry("synthetic", pair.private, password, arrayOf(certificate))
        return java.io.ByteArrayOutputStream().also { store.store(it, password) }.toByteArray()
    }
}
