// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-20
package org.openxplay.direlay.publicname

import org.bouncycastle.asn1.ASN1Encodable
import org.bouncycastle.asn1.ASN1Encoding
import org.bouncycastle.asn1.ASN1Integer
import org.bouncycastle.asn1.ASN1OctetString
import org.bouncycastle.asn1.ASN1Sequence
import org.bouncycastle.asn1.DERBitString
import org.bouncycastle.asn1.DERSequence
import org.bouncycastle.asn1.DERSet
import org.bouncycastle.asn1.pkcs.Attribute
import org.bouncycastle.asn1.pkcs.CertificationRequestInfo
import org.bouncycastle.asn1.pkcs.PKCSObjectIdentifiers
import org.bouncycastle.asn1.x500.X500Name
import org.bouncycastle.asn1.x509.AlgorithmIdentifier
import org.bouncycastle.asn1.x509.AuthorityKeyIdentifier
import org.bouncycastle.asn1.x509.Extension
import org.bouncycastle.asn1.x509.Extensions
import org.bouncycastle.asn1.x509.GeneralName
import org.bouncycastle.asn1.x509.GeneralNames
import org.bouncycastle.asn1.x509.SubjectPublicKeyInfo
import org.bouncycastle.asn1.x9.X9ObjectIdentifiers
import java.math.BigInteger
import java.security.KeyPair
import java.security.MessageDigest
import java.security.PrivateKey
import java.security.Signature
import java.security.cert.CertificateFactory
import java.security.cert.X509Certificate
import java.security.interfaces.ECPublicKey
import java.util.Base64

/** JOSE and PKCS#10 pieces of the ACME client; ASN.1 through bcprov, signing with the platform provider. */
internal object AcmeCrypto {
    fun base64Url(bytes: ByteArray): String = Base64.getUrlEncoder().withoutPadding().encodeToString(bytes)
    fun base64Url(text: String): String = base64Url(text.toByteArray(Charsets.UTF_8))
    fun sha256(bytes: ByteArray): ByteArray = MessageDigest.getInstance("SHA-256").digest(bytes)

    /** Big-endian, exactly [size] bytes: a sign byte is dropped and short values are left-padded. */
    fun unsigned(value: BigInteger, size: Int = 32): ByteArray {
        val bytes = value.toByteArray()
        val start = bytes.indexOfFirst { it != 0.toByte() }.let { if (it < 0) bytes.size else it }
        val significant = bytes.copyOfRange(start, bytes.size)
        require(significant.size <= size) { "Value does not fit" }
        return ByteArray(size - significant.size) + significant
    }

    /** RFC 7638 member order, no whitespace: the exact input of the thumbprint. */
    fun jwk(key: ECPublicKey): String {
        val x = base64Url(unsigned(key.w.affineX))
        val y = base64Url(unsigned(key.w.affineY))
        return """{"crv":"P-256","kty":"EC","x":"$x","y":"$y"}"""
    }

    fun thumbprint(key: ECPublicKey): String = base64Url(sha256(jwk(key).toByteArray(Charsets.US_ASCII)))
    fun keyAuthorization(token: String, key: ECPublicKey) = "$token.${thumbprint(key)}"
    /** The dns-01 TXT value: base64url(SHA-256(key authorization)). */
    fun dnsValue(keyAuthorization: String): String = base64Url(sha256(keyAuthorization.toByteArray(Charsets.US_ASCII)))

    /** ES256 as JWS wants it: raw r‖s, 64 bytes, converted from the platform's DER signature. */
    fun es256(key: PrivateKey, data: ByteArray): ByteArray {
        val der = Signature.getInstance("SHA256withECDSA").run { initSign(key); update(data); sign() }
        return rawSignature(der)
    }

    fun rawSignature(der: ByteArray): ByteArray {
        val sequence = ASN1Sequence.getInstance(der)
        require(sequence.size() == 2) { "Not an ECDSA signature" }
        return unsigned(ASN1Integer.getInstance(sequence.getObjectAt(0)).positiveValue) +
            unsigned(ASN1Integer.getInstance(sequence.getObjectAt(1)).positiveValue)
    }

    /** PKCS#10 with the name as the only SAN dNSName, signed SHA256withECDSA. */
    fun csr(name: String, pair: KeyPair): ByteArray {
        val extensions = Extensions(Extension.create(Extension.subjectAlternativeName, false,
            GeneralNames(GeneralName(GeneralName.dNSName, name))))
        val request = Attribute(PKCSObjectIdentifiers.pkcs_9_at_extensionRequest, DERSet(extensions))
        // A common name holds at most 64 characters; the SAN alone identifies longer names.
        val subject = if (name.length <= 64) X500Name("CN=$name") else X500Name(arrayOf<org.bouncycastle.asn1.x500.RDN>())
        val info = CertificationRequestInfo(subject, SubjectPublicKeyInfo.getInstance(pair.public.encoded), DERSet(request))
        val encoded = info.getEncoded(ASN1Encoding.DER)
        val signature = Signature.getInstance("SHA256withECDSA").run { initSign(pair.private); update(encoded); sign() }
        return DERSequence(arrayOf<ASN1Encodable>(info, AlgorithmIdentifier(X9ObjectIdentifiers.ecdsa_with_SHA256),
            DERBitString(signature))).getEncoded(ASN1Encoding.DER)
    }

    /** RFC 9773 certificate identifier: base64url(AKI keyIdentifier) "." base64url(serial number bytes). */
    fun certId(certificate: X509Certificate): String? {
        val extension = certificate.getExtensionValue(Extension.authorityKeyIdentifier.id) ?: return null
        val identifier = runCatching {
            AuthorityKeyIdentifier.getInstance(ASN1OctetString.getInstance(extension).octets).keyIdentifier
        }.getOrNull() ?: return null
        return base64Url(identifier) + "." + base64Url(certificate.serialNumber.toByteArray())
    }

    fun parseChain(pem: String): List<X509Certificate> = CertificateFactory.getInstance("X.509")
        .generateCertificates(pem.byteInputStream(Charsets.US_ASCII)).filterIsInstance<X509Certificate>()

    /** True when [name] is one of the certificate's dNSName SANs. */
    fun covers(certificate: X509Certificate, name: String): Boolean =
        certificate.subjectAlternativeNames.orEmpty().any { it.getOrNull(0) == 2 && (it.getOrNull(1) as? String).equals(name, true) }
}
