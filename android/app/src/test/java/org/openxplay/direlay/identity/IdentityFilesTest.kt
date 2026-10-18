// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-18
package org.openxplay.direlay.identity

import org.junit.Assert.*
import org.junit.Test
import org.openxplay.direlay.SyntheticIdentity
import java.io.ByteArrayOutputStream
import java.io.ByteArrayInputStream
import java.security.Signature
import java.security.KeyFactory
import java.security.cert.CertificateFactory
import java.security.spec.PKCS8EncodedKeySpec
import java.util.Base64
import java.util.zip.ZipEntry
import java.util.zip.ZipOutputStream

class IdentityFilesTest {
    private val identity = SyntheticIdentity()
    private val key = identity.pair.private.encoded
    private val certificate = identity.pkcs7
    private fun read(bytes: ByteArray) = IdentityFiles.read(bytes.inputStream())
    private fun pem(label: String, bytes: ByteArray) =
        "-----BEGIN $label-----\n${Base64.getMimeEncoder().encodeToString(bytes)}\n-----END $label-----\n".toByteArray()
    private fun zip(vararg entries: Pair<String, ByteArray>): ByteArray = ByteArrayOutputStream().also { output ->
        ZipOutputStream(output).use { zip -> entries.forEach { (name, bytes) ->
            zip.putNextEntry(ZipEntry(name)); zip.write(bytes); zip.closeEntry()
        } }
    }.toByteArray()
    private fun problem(expected: IdentityFileProblem, action: () -> Unit) {
        assertEquals(expected, assertThrows(IdentityFileException::class.java, action).problem)
    }
    /** Test validator checks possession of the matching key without requiring an Apple-issued certificate. */
    private fun validate(key: ByteArray, certificate: ByteArray) {
        val privateKey = KeyFactory.getInstance("EC").generatePrivate(PKCS8EncodedKeySpec(key))
        val publicKey = CertificateFactory.getInstance("X.509").generateCertificates(certificate.inputStream()).first().publicKey
        val signature = Signature.getInstance("SHA256withECDSA")
        signature.initSign(privateKey); signature.update(byteArrayOf(42)); val signed = signature.sign()
        signature.initVerify(publicKey); signature.update(byteArrayOf(42)); require(signature.verify(signed))
    }

    @Test fun classifiesDerAndPemKeyCertificateAndPkcs7() {
        for (bytes in listOf(key, pem("PRIVATE KEY", key))) assertEquals(IdentityKind.KEY, IdentityFiles.classify(bytes))
        for (bytes in listOf(certificate, identity.certificate.encoded, pem("PKCS7", certificate), pem("CERTIFICATE", identity.certificate.encoded)))
            assertEquals(IdentityKind.CERTIFICATE, IdentityFiles.classify(bytes))
        for (bytes in listOf(byteArrayOf(), "unrecognized".toByteArray(), pem("PRIVATE KEY", byteArrayOf(1))))
            assertEquals(IdentityKind.UNKNOWN, IdentityFiles.classify(bytes))
    }

    @Test fun bothPlainOrdersAndBothFollowUpOrders() {
        for (order in listOf(listOf(key, certificate), listOf(certificate, key))) {
            val selected = order.map(::read)
            try {
                var imports = 0
                assertNull(IdentityFiles.select(selected) { k, c -> validate(k, c); imports++ })
                assertEquals(1, imports)
                assertEquals(IdentityFiles.classify(order[0]), IdentityFiles.select(selected.take(1), validateAndImport = ::validate))
                assertNull(IdentityFiles.select(selected, followUp = true, validateAndImport = ::validate))
            } finally { selected.forEach { it.close() } }
            selected.forEach { assertTrue(it.files.all { bytes -> bytes.all { byte -> byte == 0.toByte() } }) }
        }
    }

    @Test fun unknownSingleIsNeutralAndInconclusivePairTriesBothAssignments() {
        val unknown = read(byteArrayOf(10))
        val other = read(byteArrayOf(20))
        unknown.use { first -> other.use { second ->
            assertEquals(IdentityKind.UNKNOWN, IdentityFiles.select(listOf(first)) { _, _ -> fail("Single must not validate") })
            var attempts = 0
            assertNull(IdentityFiles.select(listOf(first, second)) { k, c ->
                attempts++; require(k[0] == 20.toByte() && c[0] == 10.toByte())
            })
            assertEquals(2, attempts)
        } }
    }

    @Test fun rejectsWrongCountsSameKindsAndMismatchedPair() {
        val selected = listOf(read(key), read(key), read(certificate))
        try {
            problem(IdentityFileProblem.SELECTION) { IdentityFiles.select(emptyList(), validateAndImport = ::validate) }
            problem(IdentityFileProblem.SELECTION) { IdentityFiles.select(selected, validateAndImport = ::validate) }
            problem(IdentityFileProblem.SAME_KIND) { IdentityFiles.select(selected.take(2), validateAndImport = ::validate) }
            read(certificate).use { c -> problem(IdentityFileProblem.SAME_KIND) { selected[2].importWith(c, ::validate) } }
            read(SyntheticIdentity().pkcs7).use { c -> problem(IdentityFileProblem.INVALID_PAIR) { selected[0].importWith(c, ::validate) } }
        } finally { selected.forEach { it.close() } }
    }

    @Test fun nestedUppercaseZipAndApkNamedZipUseSignatureOnly() {
        val archive = zip("nested/IDENTITY.PK8" to key, "other/CERTIFICATE.P7B" to certificate)
        // read has no filename parameter: a provider name such as identity.apk cannot alter detection.
        read(archive).use { files -> assertTrue(files.archive); assertNull(IdentityFiles.select(listOf(files), validateAndImport = ::validate)) }
        val shortReads = object : ByteArrayInputStream(archive) {
            override fun read(bytes: ByteArray, offset: Int, length: Int) = super.read(bytes, offset, minOf(length, 1))
        }
        IdentityFiles.read(shortReads).use { assertTrue(it.archive) }
    }

    @Test fun fallbackExtensionsAndPreferredNames() {
        read(zip("a/custom.PK8" to key, "b/custom.P7B" to certificate)).use {
            assertNull(IdentityFiles.select(listOf(it), validateAndImport = ::validate))
        }
        read(zip("identity.pk8" to key, "certificate.p7b" to certificate, "other.pk8" to byteArrayOf(1), "another.pk8" to byteArrayOf(2))).use {
            assertNull(IdentityFiles.select(listOf(it), validateAndImport = ::validate))
        }
    }

    @Test fun zipNeverSearchesUnrelatedEntriesByContent() {
        problem(IdentityFileProblem.EMPTY_ARCHIVE) { read(zip("secret.key" to key, "secret.cert" to certificate)) }
        problem(IdentityFileProblem.EMPTY_ARCHIVE) { read(zip()) }
    }

    @Test fun duplicateExactAndFallbackNames() {
        for (names in listOf(listOf("identity.pk8", "folder/IDENTITY.PK8"), listOf("a.pk8", "b.pk8")))
            problem(IdentityFileProblem.DUPLICATE_KEY) { read(zip(names[0] to key, names[1] to key)) }
        for (names in listOf(listOf("certificate.p7b", "folder/CERTIFICATE.P7B"), listOf("a.p7b", "b.p7b")))
            problem(IdentityFileProblem.DUPLICATE_CERTIFICATE) { read(zip(names[0] to certificate, names[1] to certificate)) }
    }

    @Test fun zipWithOneFileAsksForTheOtherAndCompletes() {
        for ((name, bytes, kind) in listOf(
            Triple("identity.pk8", key, IdentityKind.KEY),
            Triple("certificate.p7b", certificate, IdentityKind.CERTIFICATE),
        )) {
            val other = if (kind == IdentityKind.KEY) certificate else key
            read(zip(name to bytes)).use { single -> read(other).use { plain ->
                assertEquals(kind, IdentityFiles.select(listOf(single), validateAndImport = ::validate))
                assertNull(IdentityFiles.select(listOf(single, plain), followUp = true, validateAndImport = ::validate))
                problem(IdentityFileProblem.SELECTION) { IdentityFiles.select(listOf(single, plain), validateAndImport = ::validate) }
            } }
        }
    }

    @Test fun oversizedPlainAndZipEntryAreRejected() {
        val large = ByteArray(IdentityFiles.MAX_FILE_BYTES + 1)
        problem(IdentityFileProblem.TOO_LARGE) { read(large) }
        problem(IdentityFileProblem.TOO_LARGE) { read(zip("identity.pk8" to large)) }
        read(ByteArray(IdentityFiles.MAX_FILE_BYTES)).use { assertEquals(IdentityFiles.MAX_FILE_BYTES, it.files.single().size) }
    }

    @Test fun encryptedEntryIsReported() {
        val archive = zip("identity.pk8" to key)
        archive[6] = (archive[6].toInt() or 1).toByte()
        problem(IdentityFileProblem.ENCRYPTED) { read(archive) }
    }

    @Test fun archiveEntryLimitIncludesUnrelatedFiles() {
        val archive = zip(*(0..IdentityFiles.MAX_ENTRIES).map { "unused/$it" to byteArrayOf() }.toTypedArray())
        problem(IdentityFileProblem.ARCHIVE_LIMIT) { read(archive) }
    }
    @Test fun archiveByteLimitIncludesUnmatchedExpandedData() {
        val output = ByteArrayOutputStream()
        ZipOutputStream(output).use { zip ->
            zip.putNextEntry(ZipEntry("unrelated.bin"))
            val block = ByteArray(1024 * 1024)
            repeat((IdentityFiles.MAX_ARCHIVE_BYTES / block.size).toInt() + 1) { zip.write(block) }
            zip.closeEntry()
        }
        problem(IdentityFileProblem.ARCHIVE_LIMIT) { read(output.toByteArray()) }
    }

}
