// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-20
package org.openxplay.direlay.publicname

import org.bouncycastle.asn1.ASN1Encodable
import org.bouncycastle.asn1.ASN1Integer
import org.bouncycastle.asn1.ASN1Set
import org.bouncycastle.asn1.DERSequence
import org.bouncycastle.asn1.pkcs.Attribute
import org.bouncycastle.asn1.pkcs.CertificationRequest
import org.bouncycastle.asn1.pkcs.PKCSObjectIdentifiers
import org.bouncycastle.asn1.x509.Extension
import org.bouncycastle.asn1.x509.Extensions
import org.bouncycastle.asn1.x509.GeneralName
import org.bouncycastle.asn1.x509.GeneralNames
import org.bouncycastle.asn1.x9.X9ObjectIdentifiers
import org.junit.Assert.*
import org.junit.Test
import java.math.BigInteger
import java.security.KeyFactory
import java.security.Signature
import java.security.interfaces.ECPublicKey
import java.security.spec.X509EncodedKeySpec
import java.util.Base64

/** Thumbprint and key-authorization vectors were computed once with OpenSSL 4.0.3 (see the test names). */
class AcmeCryptoTest {
    private fun publicKey(spki: String) =
        KeyFactory.getInstance("EC").generatePublic(X509EncodedKeySpec(Base64.getDecoder().decode(spki))) as ECPublicKey

    /** x = 0002179a…: two leading zero bytes, so BigInteger drops them and the JWK must pad them back. */
    private val shortX = publicKey("MFkwEwYHKoZIzj0CAQYIKoZIzj0DAQcDQgAEAAIXmreI3hzi8derkyUBO1w7cMGpfu6R+0g74qRmVSTdufuZIaB6/QTYvN33JwaUOnmTs+pQZLb2m1wG/W71qg==")
    /** x = da302976…: the high bit is set, so BigInteger adds a sign byte that the JWK must drop. */
    private val signedX = publicKey("MFkwEwYHKoZIzj0CAQYIKoZIzj0DAQcDQgAE2jApdgfccuCvUTSKCLpdAJwxkvsb4Pv9zlRWlCXfkPWGH8r2ltz2/yLoPa6ReAuT5b1dZFMoHUj0GEMhErZrrg==")
    private val token = "evaGxfADs6pSRb2LAv9IZf17Dt3juxGJ-PCt92wr-oA"

    @Test fun jwkThumbprintMatchesOpenSslForPaddedAndSignedCoordinates() {
        assertEquals("""{"crv":"P-256","kty":"EC","x":"AAIXmreI3hzi8derkyUBO1w7cMGpfu6R-0g74qRmVSQ","y":"3bn7mSGgev0E2Lzd9ycGlDp5k7PqUGS29ptcBv1u9ao"}""",
            AcmeCrypto.jwk(shortX))
        assertEquals("RDYw1V754PgcGt__FcT3rdlVWTZ4SaclTlh64H7V5fk", AcmeCrypto.thumbprint(shortX))
        assertEquals("""{"crv":"P-256","kty":"EC","x":"2jApdgfccuCvUTSKCLpdAJwxkvsb4Pv9zlRWlCXfkPU","y":"hh_K9pbc9v8i6D2ukXgLk-W9XWRTKB1I9BhDIRK2a64"}""",
            AcmeCrypto.jwk(signedX))
        assertEquals("GYEqLM0VBApsNNmRS505oNQEGhh89vGnJnIoTr0KIjs", AcmeCrypto.thumbprint(signedX))
    }

    @Test fun dnsChallengeValueMatchesOpenSsl() {
        val authorization = AcmeCrypto.keyAuthorization(token, shortX)
        assertEquals("$token.RDYw1V754PgcGt__FcT3rdlVWTZ4SaclTlh64H7V5fk", authorization)
        assertEquals("DIdSjxuYjuLoVqZ0_dijyCAqIKNMmbqo8G8vSfjtfKI", AcmeCrypto.dnsValue(authorization))
        assertEquals("FwOAVv5cTbOVdwchwHdfHnSuxtQ5IRIlHgAkmnx9lkI", AcmeCrypto.dnsValue(AcmeCrypto.keyAuthorization(token, signedX)))
    }

    @Test fun integersBecomeExactly32BigEndianBytes() {
        assertArrayEquals(ByteArray(31) + byteArrayOf(1), AcmeCrypto.unsigned(BigInteger.ONE))
        assertArrayEquals(ByteArray(32) { -1 }, AcmeCrypto.unsigned(BigInteger.ONE.shiftLeft(256).subtract(BigInteger.ONE)))
        assertThrows(IllegalArgumentException::class.java) { AcmeCrypto.unsigned(BigInteger.ONE.shiftLeft(256)) }
    }

    @Test fun es256SignaturesAreRawRAndSThatVerify() {
        val pair = TestCa.keyPair()
        repeat(64) { round ->
            val data = "message $round".toByteArray()
            val raw = AcmeCrypto.es256(pair.private, data)
            assertEquals(64, raw.size)
            val der = DERSequence(arrayOf<ASN1Encodable>(ASN1Integer(BigInteger(1, raw.copyOfRange(0, 32))),
                ASN1Integer(BigInteger(1, raw.copyOfRange(32, 64))))).encoded
            assertTrue(Signature.getInstance("SHA256withECDSA").run { initVerify(pair.public); update(data); verify(der) })
        }
        val short = DERSequence(arrayOf<ASN1Encodable>(ASN1Integer(BigInteger.valueOf(5)), ASN1Integer(BigInteger.ONE.shiftLeft(255)))).encoded
        val raw = AcmeCrypto.rawSignature(short)
        assertArrayEquals(ByteArray(31) + byteArrayOf(5), raw.copyOfRange(0, 32))
        assertEquals(0x80.toByte(), raw[32])
    }

    @Test fun csrCarriesOnlyTheNameAndIsSignedByTheCertificateKey() {
        val pair = TestCa.keyPair()
        val request = CertificationRequest.getInstance(AcmeCrypto.csr("myrelay.duckdns.org", pair))
        val info = request.certificationRequestInfo
        assertEquals("CN=myrelay.duckdns.org", info.subject.toString())
        assertArrayEquals(pair.public.encoded, info.subjectPublicKeyInfo.encoded)
        assertEquals(X9ObjectIdentifiers.ecdsa_with_SHA256, request.signatureAlgorithm.algorithm)
        assertNull(request.signatureAlgorithm.parameters)
        assertTrue(Signature.getInstance("SHA256withECDSA").run {
            initVerify(pair.public); update(info.encoded); verify(request.signature.bytes)
        })
        val attribute = Attribute.getInstance(info.attributes.getObjectAt(0))
        assertEquals(PKCSObjectIdentifiers.pkcs_9_at_extensionRequest, attribute.attrType)
        val extensions = Extensions.getInstance((attribute.attrValues as ASN1Set).getObjectAt(0))
        val names = GeneralNames.getInstance(extensions.getExtensionParsedValue(Extension.subjectAlternativeName)).names
        assertEquals(listOf(GeneralName(GeneralName.dNSName, "myrelay.duckdns.org")), names.toList())
        // Common names hold at most 64 characters; a longer name is in the SAN only.
        val long = "a".repeat(60) + ".dynv6.net"
        assertEquals("", CertificationRequest.getInstance(AcmeCrypto.csr(long, pair)).certificationRequestInfo.subject.toString())
    }

    @Test fun ariCertIdIsAuthorityKeyIdAndSerialBytes() {
        // RFC 9773 section 4.1's example: AKI 69:88:5B:6B:… and serial 0x87654321 (DER content 00 87 65 43 21).
        val ca = TestCa()
        val leaf = ca.leaf("myrelay.duckdns.org", TestCa.keyPair().public, serial = BigInteger("87654321", 16),
            authorityKeyId = BigInteger("69885B6B87464041E1B37B847BA0AE2CDE01C8D4", 16).toByteArray())
        assertEquals("aYhba4dGQEHhs3uEe6CuLN4ByNQ.AIdlQyE", AcmeCrypto.certId(leaf))
        assertTrue(AcmeCrypto.covers(leaf, "MyRelay.duckdns.org"))
        assertFalse(AcmeCrypto.covers(leaf, "other.duckdns.org"))
        assertNull("no AKI, no certID", AcmeCrypto.certId(ca.certificate))
    }
}
