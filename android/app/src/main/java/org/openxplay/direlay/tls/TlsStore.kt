// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-14
package org.openxplay.direlay.tls

import org.openxplay.direlay.storage.MaterialStatus
import org.openxplay.direlay.storage.PrivateStore
import java.io.File
import java.security.KeyStore
import java.security.SecureRandom
import javax.net.ssl.KeyManagerFactory
import javax.net.ssl.SSLContext

class TlsStore(noBackupFilesDir: File, private val onChanged: () -> Unit = {}) {
    private val store = PrivateStore(noBackupFilesDir, "tls")
    private val generated = PrivateStore(noBackupFilesDir, "tls-generated")

    /** Imported material always wins, even if invalid; never silently replace a user's certificate. */
    @Synchronized fun ensureDefault(addresses: Set<String>, hostNames: Set<String> = emptySet()): Boolean {
        if (java.nio.file.Files.exists(store.directory.toPath(), java.nio.file.LinkOption.NOFOLLOW_LINKS)) return false
        val names = hostNames + "openxplay-android.local"
        val canonical = "dns:" + names.sorted().joinToString(",") + "\n" + addresses.sorted().joinToString("\n")
        val metadata = File(generated.directory, "addresses")
        if (metadata.isFile && metadata.readText() == canonical &&
            generated.status { load(it) } == MaterialStatus.Installed) return false
        val password = ByteArray(32).also { SecureRandom().nextBytes(it) }
            .joinToString("") { "%02x".format(it.toInt() and 255) }.toCharArray()
        try {
            val data = SelfSignedCertificate.pkcs12(addresses, password, names)
            generated.replace {
                PrivateStore.write(it, "certificate.p12", data)
                PrivateStore.write(it, "password", String(password).toByteArray(Charsets.UTF_8))
                PrivateStore.write(it, "addresses", canonical.toByteArray(Charsets.UTF_8))
                load(it)
            }
        } finally { password.fill('\u0000') }
        return true
    }

    @Synchronized fun webContext(addresses: Set<String>, hostNames: Set<String> = emptySet()): SSLContext {
        ensureDefault(addresses, hostNames)
        return checkNotNull(sslContext()) { "TLS certificate unavailable" }
    }

    @Synchronized fun fingerprint(): String? {
        val directory = if (hasImported()) store.directory else generated.directory
        if (!directory.exists()) return null
        val password = File(directory, "password").readText().toCharArray()
        try {
            val keys = KeyStore.getInstance("PKCS12")
            File(directory, "certificate.p12").inputStream().use { keys.load(it, password) }
            val alias = keys.aliases().toList().first { keys.isKeyEntry(it) }
            return java.security.MessageDigest.getInstance("SHA-256").digest(keys.getCertificate(alias).encoded)
                .joinToString(":") { "%02X".format(it.toInt() and 255) }
        } finally { password.fill('\u0000') }
    }

    @Synchronized fun hasImported(): Boolean =
        java.nio.file.Files.exists(store.directory.toPath(), java.nio.file.LinkOption.NOFOLLOW_LINKS)

    @Synchronized fun status(): MaterialStatus = (if (hasImported()) store else generated).status { load(it) }

    @Synchronized fun importFrom(pkcs12: ByteArray, password: CharArray) {
        require(pkcs12.size in 1..MAX_FILE_BYTES) { "PKCS12 file must contain 1–1048576 bytes" }
        require(password.size <= MAX_PASSWORD_CHARS) { "Password is too long" }
        store.replace { staging ->
            PrivateStore.write(staging, "certificate.p12", pkcs12)
            val encoded = String(password).toByteArray(Charsets.UTF_8)
            try { PrivateStore.write(staging, "password", encoded) } finally { encoded.fill(0) }
            try { load(staging) }
            catch (_: Exception) { throw IllegalArgumentException("Invalid PKCS12 file, incorrect password, or missing private key") }
        }
        onChanged()
    }

    @Synchronized fun sslContext(): SSLContext? {
        val selected = if (hasImported()) store else generated
        if (!java.nio.file.Files.exists(selected.directory.toPath(), java.nio.file.LinkOption.NOFOLLOW_LINKS)) return null
        return try { load(selected.directory.canonicalFile) }
        catch (_: Exception) { throw IllegalStateException("Stored TLS files failed validation; import again or remove them") }
    }

    @Synchronized fun remove() { store.remove(); onChanged() }

    private fun load(directory: File): SSLContext {
        val certificate = File(directory, "certificate.p12")
        val passwordFile = File(directory, "password")
        require(certificate.length() in 1..MAX_FILE_BYTES.toLong())
        require(passwordFile.isFile && passwordFile.length() <= MAX_PASSWORD_CHARS * 4L)
        val password = passwordFile.readText(Charsets.UTF_8).toCharArray()
        try {
            val keys = KeyStore.getInstance("PKCS12")
            certificate.inputStream().use { keys.load(it, password) }
            require(keys.aliases().toList().any { alias ->
                keys.isKeyEntry(alias) && keys.getKey(alias, password) is java.security.PrivateKey &&
                    !keys.getCertificateChain(alias).isNullOrEmpty()
            }) { "PKCS12 contains no private key with a certificate chain" }
            val managers = KeyManagerFactory.getInstance(KeyManagerFactory.getDefaultAlgorithm())
            managers.init(keys, password)
            return SSLContext.getInstance("TLS").apply { init(managers.keyManagers, null, SecureRandom()) }
        } finally { password.fill('\u0000') }
    }

    companion object {
        const val MAX_FILE_BYTES = 1024 * 1024
        const val MAX_PASSWORD_CHARS = 4096
        fun port(tls: Boolean): Int = if (tls) 8443 else 8080
    }
}
