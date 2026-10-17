// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-14
package org.openxplay.direlay.tls

import java.math.BigInteger
import java.security.KeyPairGenerator
import java.security.KeyStore
import java.security.SecureRandom
import java.security.Signature
import java.security.cert.CertificateFactory
import java.security.spec.ECGenParameterSpec
import java.util.Date
import org.bouncycastle.asn1.*
import org.bouncycastle.asn1.x500.X500Name
import org.bouncycastle.asn1.x509.*
import org.bouncycastle.asn1.x9.X9ObjectIdentifiers

/** Uses only bcprov's ASN.1 types; signing stays with the platform's EC implementation. */
internal object SelfSignedCertificate {
    fun pkcs12(addresses: Set<String>, password: CharArray, hostNames: Set<String> = setOf("openxplay-android.local")): ByteArray {
        val pair = KeyPairGenerator.getInstance("EC").apply {
            initialize(ECGenParameterSpec("secp256r1"))
        }.generateKeyPair()
        val algorithm = AlgorithmIdentifier(X9ObjectIdentifiers.ecdsa_with_SHA256)
        val name = X500Name("CN=OpenXPlay Relay")
        val names = (hostNames + "localhost" + "openxplay-android.local").sorted().map { GeneralName(GeneralName.dNSName, it) } +
            addresses.sorted().map { GeneralName(GeneralName.iPAddress, it) }
        val tbs = V3TBSCertificateGenerator().apply {
            setSerialNumber(ASN1Integer(BigInteger(159, SecureRandom()).add(BigInteger.ONE)))
            setSignature(algorithm)
            setIssuer(name)
            setSubject(name)
            setStartDate(Time(Date(System.currentTimeMillis() - 86_400_000L)))
            setEndDate(Time(Date(System.currentTimeMillis() + 10L * 365 * 86_400_000L)))
            setSubjectPublicKeyInfo(SubjectPublicKeyInfo.getInstance(pair.public.encoded))
            setExtensions(Extensions(arrayOf(
                Extension.create(Extension.subjectAlternativeName, false, GeneralNames(names.toTypedArray())),
                Extension.create(Extension.basicConstraints, true, BasicConstraints(false)),
                Extension.create(Extension.keyUsage, true, KeyUsage(KeyUsage.digitalSignature)),
                Extension.create(Extension.extendedKeyUsage, false, ExtendedKeyUsage(KeyPurposeId.id_kp_serverAuth)),
            )))
        }.generateTBSCertificate()
        val signature = Signature.getInstance("SHA256withECDSA").run {
            initSign(pair.private); update(tbs.encoded); sign()
        }
        val certificate = CertificateFactory.getInstance("X.509").generateCertificate(
            DERSequence(arrayOf<ASN1Encodable>(tbs, algorithm, DERBitString(signature))).encoded.inputStream())
        val keys = KeyStore.getInstance("PKCS12").apply {
            load(null, password)
            setKeyEntry("relay", pair.private, password, arrayOf(certificate))
        }
        return java.io.ByteArrayOutputStream().also { keys.store(it, password) }.toByteArray()
    }
}
