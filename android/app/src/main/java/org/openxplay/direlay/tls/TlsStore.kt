// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-14
package org.openxplay.direlay.tls

import org.openxplay.direlay.storage.MaterialStatus
import org.openxplay.direlay.storage.PrivateStore
import java.io.File
import java.security.KeyFactory
import java.security.KeyStore
import java.security.PrivateKey
import java.security.SecureRandom
import java.security.Signature
import java.security.cert.CertificateFactory
import java.security.cert.X509Certificate
import java.security.spec.PKCS8EncodedKeySpec
import javax.net.ssl.KeyManager
import javax.net.ssl.KeyManagerFactory
import javax.net.ssl.SSLContext
import javax.net.ssl.X509KeyManager

/** The public name's Let's Encrypt certificate: [chain] starts with the leaf for [name]. */
class AcmeMaterial(val name: String, val chain: List<X509Certificate>, val key: PrivateKey) {
    val leaf: X509Certificate get() = chain.first()
    val fingerprint: String get() = java.security.MessageDigest.getInstance("SHA-256").digest(leaf.encoded)
        .joinToString(":") { "%02X".format(it.toInt() and 255) }
}

class TlsStore(noBackupFilesDir: File, private val now: () -> Long = System::currentTimeMillis,
    private val onChanged: () -> Unit = {}) {
    private val store = PrivateStore(noBackupFilesDir, "tls")
    private val generated = PrivateStore(noBackupFilesDir, "tls-generated")
    /** Separate from the imported material, so Developer → Remove never deletes it. */
    private val acme = PrivateStore(noBackupFilesDir, "tls-acme")
    /** The name the public name service serves; null while it is off. */
    private var acmeName: String? = null
    /** Every install is a new generation directory, so its canonical path identifies the parsed material. */
    private var acmeCache: Pair<File, AcmeMaterial?>? = null
    private data class MaterialIdentity(val directory: File, val certificateLength: Long, val certificateModified: Long,
        val passwordLength: Long, val passwordModified: Long)
    private data class ParsedMaterial(val managers: Array<KeyManager>, val fingerprint: String,
        val validity: List<Pair<Long, Long>>)
    private var materialCache: Pair<MaterialIdentity, ParsedMaterial>? = null
    internal var materialLoads = 0
        private set

    /** Lock-free on purpose: the log recorder must never wait for a keystore load holding this store's lock. */
    fun logPasswords(): List<String> = listOf(store.directory, generated.directory).mapNotNull {
        val file = File(it, "password")
        if (file.isFile && file.length() <= MAX_PASSWORD_CHARS * 4L) file.readText(Charsets.UTF_8) else null
    }

    /** Imported material always wins, even if invalid; never silently replace a user's certificate. */
    @Synchronized fun ensureDefault(addresses: Set<String>, hostNames: Set<String> = emptySet()): Boolean {
        if (java.nio.file.Files.exists(store.directory.toPath(), java.nio.file.LinkOption.NOFOLLOW_LINKS)) return false
        val current = hostNames + "openxplay-android.local"
        val metadata = File(generated.directory, "addresses")
        val previous = if (metadata.isFile) metadata.readLines() else emptyList()
        val previousNames = previous.firstOrNull()?.takeIf { it.startsWith("dns:") }
            ?.removePrefix("dns:")?.split(',')?.filter { it.isNotBlank() }.orEmpty()
        val previousAddresses = previous.drop(1).filter { it.isNotBlank() }
        val reason = when {
            previous.isEmpty() -> "none"
            !previousNames.containsAll(current) -> "name"
            !previousAddresses.containsAll(addresses) -> "address"
            generated.status { load(it, validateDates = true) } != MaterialStatus.Installed -> "invalid"
            else -> return false
        }
        // Names and addresses that come and go with a network stay covered, so flapping never regenerates.
        val names = (current.sorted() + previousNames).distinct().take(8).toSet()
        val covered = (addresses.sorted() + previousAddresses).distinct().take(16).toSet()
        android.util.Log.i("OpenXPlay Relay", "TLS certificate regenerated: $reason, ${names.size} names, ${covered.size} addresses")
        val canonical = "dns:" + names.joinToString(",") + "\n" + covered.joinToString("\n")
        val password = ByteArray(32).also { SecureRandom().nextBytes(it) }
            .joinToString("") { "%02x".format(it.toInt() and 255) }.toCharArray()
        try {
            val data = SelfSignedCertificate.pkcs12(covered, password, names)
            materialCache = null
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

    /** The imported or generated certificate; the ACME certificate has its own [AcmeMaterial.fingerprint]. */
    @Synchronized fun fingerprint(): String? {
        val directory = if (hasImported()) store.directory else generated.directory
        if (!directory.exists()) return null
        return parsedMaterial(directory).fingerprint
    }

    /** Changes whenever a new connection would see a different certificate: servers restart on a change. */
    @Synchronized fun revision(): String? {
        val base = fingerprint()
        return servedAcme()?.let { "$base+${it.fingerprint}" } ?: base
    }

    @Synchronized fun hasImported(): Boolean =
        java.nio.file.Files.exists(store.directory.toPath(), java.nio.file.LinkOption.NOFOLLOW_LINKS)

    @Synchronized fun status(): MaterialStatus = (if (hasImported()) store else generated).status { load(it) }

    @Synchronized fun importFrom(pkcs12: ByteArray, password: CharArray) {
        require(pkcs12.size in 1..MAX_FILE_BYTES) { "PKCS12 file must contain 1–1048576 bytes" }
        require(password.size <= MAX_PASSWORD_CHARS) { "Password is too long" }
        materialCache = null
        store.replace { staging ->
            PrivateStore.write(staging, "certificate.p12", pkcs12)
            val encoded = String(password).toByteArray(Charsets.UTF_8)
            try { PrivateStore.write(staging, "password", encoded) } finally { encoded.fill(0) }
            try { load(staging) }
            catch (_: Exception) { throw IllegalArgumentException("Invalid PKCS12 file, incorrect password, or missing private key") }
        }
        onChanged()
    }

    /** Only user-imported material belongs in a privacy configuration file. */
    @Synchronized fun importedMaterial(): Pair<List<X509Certificate>, PrivateKey>? {
        if (!hasImported()) return null
        val password = File(store.directory, "password").readText(Charsets.UTF_8).toCharArray()
        try {
            val keys = KeyStore.getInstance("PKCS12")
            File(store.directory, "certificate.p12").inputStream().use { keys.load(it, password) }
            val alias = keys.aliases().toList().first { keys.isKeyEntry(it) }
            return keys.getCertificateChain(alias).map { it as X509Certificate } to (keys.getKey(alias, password) as PrivateKey)
        } finally { password.fill('\u0000') }
    }

    @Synchronized fun importMaterial(chain: List<X509Certificate>, key: PrivateKey) {
        materialCache = null
        val password = ByteArray(32).also { SecureRandom().nextBytes(it) }
            .let { java.util.Base64.getEncoder().encodeToString(it) }.toCharArray()
        try {
            val keys = KeyStore.getInstance("PKCS12").apply { load(null, password) }
            keys.setKeyEntry("imported", key, password, chain.toTypedArray())
            val bytes = java.io.ByteArrayOutputStream().also { keys.store(it, password) }.toByteArray()
            try { importFrom(bytes, password) } finally { bytes.fill(0) }
        } finally { password.fill('\u0000') }
    }

    /**
     * Today's certificate (imported, else generated). While the public name is served, the key manager
     * hands its Let's Encrypt chain to clients that ask for that name (SNI) and today's to everyone else.
     */
    @Synchronized fun sslContext(): SSLContext? {
        val selected = if (hasImported()) store else generated
        if (!java.nio.file.Files.exists(selected.directory.toPath(), java.nio.file.LinkOption.NOFOLLOW_LINKS)) return null
        val managers = try { keyManagers(selected.directory.canonicalFile) }
        catch (_: Exception) { throw IllegalStateException("Stored TLS files failed validation; import again or remove them") }
        val served = servedAcme()
        val chosen = if (served == null) managers else managers.map { manager ->
            if (manager is X509KeyManager) SniKeyManager(served.name, served.key, served.chain, manager, now) else manager
        }.toTypedArray()
        return SSLContext.getInstance("TLS").apply { init(chosen, null, SecureRandom()) }
    }

    /** Removes only imported material; the generated certificate and the ACME certificate stay. */
    @Synchronized fun remove() { materialCache = null; store.remove(); onChanged() }

    /** The ACME certificate, or null when there is none or it fails validation. */
    @Synchronized fun acmeMaterial(): AcmeMaterial? {
        if (!java.nio.file.Files.exists(acme.directory.toPath(), java.nio.file.LinkOption.NOFOLLOW_LINKS)) return null
        val directory = acme.directory.canonicalFile
        acmeCache?.takeIf { it.first == directory }?.let { return it.second }
        return (try { readAcme(directory) } catch (_: Exception) { null }).also { acmeCache = directory to it }
    }

    /** Validated before it replaces the previous ACME certificate in one atomic switch. */
    @Synchronized fun installAcme(name: String, chain: List<X509Certificate>, key: PrivateKey) {
        require(chain.isNotEmpty()) { "Empty certificate chain" }
        acme.replace { staging ->
            PrivateStore.write(staging, ACME_NAME, name.toByteArray(Charsets.US_ASCII))
            PrivateStore.write(staging, ACME_CHAIN, pem(chain).toByteArray(Charsets.US_ASCII))
            val encoded = key.encoded
            try { PrivateStore.write(staging, ACME_KEY, encoded) } finally { encoded.fill(0) }
            require(readAcme(staging).name == name) { "Stored certificate does not match" }
        }
        acmeCache = null
        onChanged()
    }

    @Synchronized fun removeAcme() {
        val existed = java.nio.file.Files.exists(acme.directory.toPath(), java.nio.file.LinkOption.NOFOLLOW_LINKS)
        acme.remove()
        acmeCache = null
        if (existed) onChanged()
    }

    /** Serve the ACME certificate for SNI [name] while it is valid; null serves only today's certificate. */
    @Synchronized fun serveAcme(name: String?) {
        if (name == acmeName) return
        val before = servedAcme()?.fingerprint
        acmeName = name
        if (servedAcme()?.fingerprint != before) onChanged()
    }

    private fun servedAcme(): AcmeMaterial? {
        val name = acmeName ?: return null
        val material = acmeMaterial()?.takeIf { it.name == name } ?: return null
        val time = now()
        return material.takeIf { time >= it.leaf.notBefore.time && time < it.leaf.notAfter.time }
    }

    private fun readAcme(directory: File): AcmeMaterial {
        val nameFile = File(directory, ACME_NAME)
        val chainFile = File(directory, ACME_CHAIN)
        val keyFile = File(directory, ACME_KEY)
        require(nameFile.length() in 1..253 && chainFile.length() in 1..MAX_FILE_BYTES && keyFile.length() in 1..4096)
        val name = nameFile.readText(Charsets.US_ASCII)
        val chain = chainFile.inputStream().buffered().use { input ->
            CertificateFactory.getInstance("X.509").generateCertificates(input).filterIsInstance<X509Certificate>()
        }
        require(chain.isNotEmpty())
        val encoded = keyFile.readBytes()
        val key = try { KeyFactory.getInstance("EC").generatePrivate(PKCS8EncodedKeySpec(encoded)) } finally { encoded.fill(0) }
        val leaf = chain.first()
        require(leaf.subjectAlternativeNames.orEmpty().any { it.getOrNull(0) == 2 && (it.getOrNull(1) as? String).equals(name, ignoreCase = true) })
        // The key must belong to the leaf: sign a random message and check it with the leaf's public key.
        val probe = ByteArray(32).also { SecureRandom().nextBytes(it) }
        val signature = Signature.getInstance("SHA256withECDSA").run { initSign(key); update(probe); sign() }
        require(Signature.getInstance("SHA256withECDSA").run { initVerify(leaf.publicKey); update(probe); verify(signature) })
        return AcmeMaterial(name, chain, key)
    }

    private fun load(directory: File, validateDates: Boolean = false): SSLContext =
        SSLContext.getInstance("TLS").apply { init(keyManagers(directory, validateDates), null, SecureRandom()) }

    private fun keyManagers(directory: File, validateDates: Boolean = false): Array<KeyManager> {
        val material = parsedMaterial(directory)
        if (validateDates) {
            val time = now()
            require(material.validity.all { time >= it.first && time <= it.second }) { "TLS certificate is outside its validity period" }
        }
        return material.managers
    }

    private fun parsedMaterial(directory: File): ParsedMaterial {
        val canonical = directory.canonicalFile
        val certificate = File(canonical, "certificate.p12")
        val passwordFile = File(canonical, "password")
        val identity = MaterialIdentity(canonical, certificate.length(), certificate.lastModified(),
            passwordFile.length(), passwordFile.lastModified())
        materialCache?.takeIf { it.first == identity && passwordFile.isFile }?.let { return it.second }
        materialCache = null
        require(identity.certificateLength in 1..MAX_FILE_BYTES.toLong())
        require(passwordFile.isFile && identity.passwordLength <= MAX_PASSWORD_CHARS * 4L)
        val password = passwordFile.readText(Charsets.UTF_8).toCharArray()
        try {
            val keys = KeyStore.getInstance("PKCS12")
            certificate.inputStream().use { materialLoads++; keys.load(it, password) }
            val aliases = keys.aliases().toList().filter { keys.isKeyEntry(it) }
            require(aliases.any { alias ->
                keys.getKey(alias, password) is java.security.PrivateKey &&
                    !keys.getCertificateChain(alias).isNullOrEmpty()
            }) { "PKCS12 contains no private key with a certificate chain" }
            val fingerprint = java.security.MessageDigest.getInstance("SHA-256").digest(keys.getCertificate(aliases.first()).encoded)
                .joinToString(":") { "%02X".format(it.toInt() and 255) }
            val validity = aliases.flatMap { keys.getCertificateChain(it).filterIsInstance<X509Certificate>() }
                .map { it.notBefore.time to it.notAfter.time }
            val managers = KeyManagerFactory.getInstance(KeyManagerFactory.getDefaultAlgorithm())
            managers.init(keys, password)
            return ParsedMaterial(managers.keyManagers, fingerprint, validity).also { materialCache = identity to it }
        } finally { password.fill('\u0000') }
    }

    companion object {
        const val MAX_FILE_BYTES = 1024 * 1024
        const val MAX_PASSWORD_CHARS = 4096
        private const val ACME_NAME = "name"
        private const val ACME_CHAIN = "chain.pem"
        private const val ACME_KEY = "key.pk8"
        fun port(tls: Boolean): Int = if (tls) 8443 else 8080

        fun pem(chain: List<X509Certificate>): String = chain.joinToString("") { certificate ->
            "-----BEGIN CERTIFICATE-----\n" +
                java.util.Base64.getMimeEncoder(64, "\n".toByteArray()).encodeToString(certificate.encoded) +
                "\n-----END CERTIFICATE-----\n"
        }
    }
}
