// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-18
package org.openxplay.direlay.identity

import java.io.InputStream
import java.security.KeyFactory
import java.security.cert.CertificateFactory
import java.security.spec.PKCS8EncodedKeySpec
import java.util.Base64
import java.util.zip.ZipException
import java.util.zip.ZipInputStream

enum class IdentityKind { KEY, CERTIFICATE, UNKNOWN }
enum class IdentityFileProblem { SELECTION, SAME_KIND, INVALID_PAIR, EMPTY_ARCHIVE, DUPLICATE_KEY, DUPLICATE_CERTIFICATE, ENCRYPTED, TOO_LARGE, ARCHIVE_LIMIT, BAD_ARCHIVE }
class IdentityFileException(val problem: IdentityFileProblem) : IllegalArgumentException(problem.name)

/** Owns the selected bytes. Callers close it on success, failure and cancellation. */
class IdentityFiles(val files: List<ByteArray>, val archive: Boolean = false) : AutoCloseable {
    override fun close() { files.forEach { it.fill(0) } }
    val followUp: IdentityKind get() = classify(files.single())

    fun importWith(other: IdentityFiles? = null, validateAndImport: (ByteArray, ByteArray) -> Unit) {
        val all = files + other?.files.orEmpty()
        if (all.size != 2) fail(IdentityFileProblem.SELECTION)
        val first = classify(all[0]); val second = classify(all[1])
        if (first != IdentityKind.UNKNOWN && first == second) fail(IdentityFileProblem.SAME_KIND)
        val reversed = first == IdentityKind.CERTIFICATE || second == IdentityKind.KEY
        val key = all[if (reversed) 1 else 0]; val certificate = all[if (reversed) 0 else 1]
        try { validateAndImport(key, certificate); return }
        catch (_: Exception) { }
        if (first == IdentityKind.UNKNOWN || second == IdentityKind.UNKNOWN) {
            try { validateAndImport(certificate, key); return }
            catch (_: Exception) { }
        }
        fail(IdentityFileProblem.INVALID_PAIR)
    }

    companion object {
        const val MAX_FILE_BYTES = 16 * 1024
        const val MAX_ENTRIES = 20_000
        const val MAX_ARCHIVE_BYTES = 512L * 1024 * 1024
        private fun fail(problem: IdentityFileProblem): Nothing = throw IdentityFileException(problem)

        /** Returns the recognized single-file type, or null after a complete validated import. */
        fun select(selected: List<IdentityFiles>, followUp: Boolean = false,
            validateAndImport: (ByteArray, ByteArray) -> Unit,
        ): IdentityKind? {
            if (selected.isEmpty() || selected.size > 2 ||
                (selected.size > 1 && selected.any { it.archive } && !followUp) ||
                (followUp && (selected.size != 2 || selected.any { it.files.size != 1 }))) fail(IdentityFileProblem.SELECTION)
            if (selected.size == 1 && selected[0].files.size == 1) return selected[0].followUp
            selected[0].importWith(selected.getOrNull(1), validateAndImport)
            return null
        }

        fun classify(bytes: ByteArray): IdentityKind {
            var decoded: ByteArray? = null
            try {
                val pem = bytes.firstOrNull { it.toInt() > 32 } == '-'.code.toByte()
                val data = if (pem) {
                    val body = bytes.toString(Charsets.US_ASCII).trim().lineSequence().filterNot { it.startsWith("-----") }.joinToString("")
                    Base64.getDecoder().decode(body).also { decoded = it }
                } else bytes
                if (runCatching { KeyFactory.getInstance("EC").generatePrivate(PKCS8EncodedKeySpec(data)) }.isSuccess) return IdentityKind.KEY
                if (runCatching { CertificateFactory.getInstance("X.509").generateCertificates(data.inputStream()).isNotEmpty() }.getOrDefault(false)) return IdentityKind.CERTIFICATE
                return IdentityKind.UNKNOWN
            } catch (_: Exception) { return IdentityKind.UNKNOWN }
            finally { decoded?.fill(0) }
        }

        /** Signature detection ignores MIME type and filename; archive lookup uses names only. */
        fun read(input: InputStream): IdentityFiles = input.use { source ->
            val signature = ByteArray(4)
            try {
                var count = 0
                while (count < signature.size) {
                    val read = source.read(signature, count, signature.size - count)
                    if (read < 0) break
                    count += read
                }
                val zip = count == 4 && signature[0] == 0x50.toByte() && signature[1] == 0x4b.toByte() &&
                    ((signature[2] == 3.toByte() && signature[3] == 4.toByte()) ||
                        (signature[2] == 5.toByte() && signature[3] == 6.toByte()) ||
                        (signature[2] == 7.toByte() && signature[3] == 8.toByte()))
                val replay = java.io.SequenceInputStream(java.io.ByteArrayInputStream(signature, 0, count), source)
                if (!zip) IdentityFiles(listOf(readSmall(replay))) else readZip(replay)
            } finally { signature.fill(0) }
        }

        private fun readSmall(input: InputStream): ByteArray {
            val buffer = ByteArray(MAX_FILE_BYTES + 1)
            try {
                var size = 0
                while (size < buffer.size) {
                    val count = input.read(buffer, size, buffer.size - size)
                    if (count < 0) break
                    size += count
                }
                if (size > MAX_FILE_BYTES) fail(IdentityFileProblem.TOO_LARGE)
                return buffer.copyOf(size)
            } finally { buffer.fill(0) }
        }

        private fun readZip(input: InputStream): IdentityFiles {
            val exact = arrayOf(mutableListOf<ByteArray>(), mutableListOf<ByteArray>())
            val fallback = arrayOf(mutableListOf<ByteArray>(), mutableListOf<ByteArray>())
            val retained = mutableListOf<ByteArray>()
            val buffer = ByteArray(4096)
            var success = false
            try {
                val limited = object : java.io.FilterInputStream(input) {
                    var compressed = 0L
                    override fun read(bytes: ByteArray, offset: Int, length: Int): Int {
                        val count = super.read(bytes, offset, length)
                        if (count > 0) compressed += count
                        if (compressed > MAX_ARCHIVE_BYTES) fail(IdentityFileProblem.ARCHIVE_LIMIT)
                        return count
                    }
                }
                ZipInputStream(limited).use { zip ->
                    var entries = 0; var total = 0L
                    while (true) {
                        val entry = zip.nextEntry ?: break
                        if (++entries > MAX_ENTRIES) fail(IdentityFileProblem.ARCHIVE_LIMIT)
                        val name = entry.name.substringAfterLast('/').lowercase(java.util.Locale.ROOT)
                        val kind = when { entry.isDirectory -> -1; name.endsWith(".pk8") -> 0; name.endsWith(".p7b") -> 1; else -> -1 }
                        val preferred = name == "identity.pk8" || name == "certificate.p7b"
                        val scratch = if (kind >= 0) ByteArray(MAX_FILE_BYTES + 1) else null
                        var size = 0
                        try {
                            while (true) {
                                val count = zip.read(buffer, 0, if (scratch == null) buffer.size else minOf(buffer.size, MAX_FILE_BYTES + 1 - size))
                                if (count < 0) break
                                total += count
                                if (total > MAX_ARCHIVE_BYTES) fail(IdentityFileProblem.ARCHIVE_LIMIT)
                                if (scratch != null) {
                                    if (size + count > MAX_FILE_BYTES) fail(IdentityFileProblem.TOO_LARGE)
                                    buffer.copyInto(scratch, size, 0, count); size += count
                                }
                            }
                            if (scratch != null) {
                                val candidates = if (preferred) exact[kind] else fallback[kind]
                                // Two candidates suffice to report ambiguity, even in a very large archive.
                                if (candidates.size < 2) scratch.copyOf(size).also { candidates.add(it); retained.add(it) }
                            }
                        } finally { scratch?.fill(0) }
                    }
                }
                val selected = (0..1).mapNotNull { kind ->
                    val candidates = exact[kind].ifEmpty { fallback[kind] }
                    if (candidates.size > 1) fail(if (kind == 0) IdentityFileProblem.DUPLICATE_KEY else IdentityFileProblem.DUPLICATE_CERTIFICATE)
                    candidates.singleOrNull()
                }
                if (selected.isEmpty()) fail(IdentityFileProblem.EMPTY_ARCHIVE)
                retained.filter { candidate -> selected.none { it === candidate } }.forEach { it.fill(0) }
                success = true
                return IdentityFiles(selected, archive = true)
            } catch (failure: ZipException) {
                fail(if (failure.message.orEmpty().contains("encrypt", ignoreCase = true)) IdentityFileProblem.ENCRYPTED else IdentityFileProblem.BAD_ARCHIVE)
            } finally { buffer.fill(0); if (!success) retained.forEach { it.fill(0) } }
        }
    }
}
