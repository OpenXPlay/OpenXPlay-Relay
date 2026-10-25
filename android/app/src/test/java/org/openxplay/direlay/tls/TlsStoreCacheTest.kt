// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-25
package org.openxplay.direlay.tls

import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder
import org.openxplay.direlay.SyntheticIdentity
import org.openxplay.direlay.storage.MaterialStatus
import java.io.File
import java.nio.file.Files
import java.nio.file.attribute.FileTime
import java.security.KeyStore
import java.security.cert.X509Certificate

class TlsStoreCacheTest {
    @get:Rule val temporary = TemporaryFolder()
    private val addresses = setOf("192.168.1.2")

    @Test fun repeatedRevisionAndContextsReuseImportedMaterial() {
        val root = temporary.newFolder()
        TlsStore(root).importFrom(SyntheticIdentity().pkcs12(charArrayOf()), charArrayOf())
        val store = TlsStore(root)
        val revision = store.revision()
        assertNotNull(revision)
        assertEquals(1, store.materialLoads)
        assertEquals(revision, store.revision())
        assertEquals(revision, store.fingerprint())
        assertEquals(MaterialStatus.Installed, store.status())
        assertNotNull(store.sslContext())
        assertNotNull(store.webContext(addresses))
        assertEquals(1, store.materialLoads)
    }

    @Test fun repeatedEnsureDefaultAndRevisionReuseGeneratedMaterial() {
        val root = temporary.newFolder()
        val store = TlsStore(root)
        assertTrue(store.ensureDefault(addresses))
        assertEquals(1, store.materialLoads)
        val revision = store.revision()
        assertFalse(store.ensureDefault(addresses))
        assertEquals(revision, store.revision())
        assertNotNull(store.webContext(addresses))
        assertEquals(1, store.materialLoads)
        val reopened = TlsStore(root)
        assertFalse(reopened.ensureDefault(addresses))
        assertFalse(reopened.ensureDefault(addresses))
        assertEquals(revision, reopened.revision())
        assertEquals(1, reopened.materialLoads)
    }

    @Test fun importsReplacementAndRemovalDropCachedMaterial() {
        val store = TlsStore(temporary.newFolder())
        assertTrue(store.ensureDefault(addresses))
        val generated = store.revision()
        store.importFrom(SyntheticIdentity().pkcs12(charArrayOf()), charArrayOf())
        val imported = store.revision()
        assertNotEquals(generated, imported)
        assertEquals(2, store.materialLoads)
        val replacement = SyntheticIdentity()
        store.importMaterial(listOf(replacement.certificate as X509Certificate), replacement.pair.private)
        assertNotEquals(imported, store.revision())
        assertEquals(3, store.materialLoads)
        store.remove()
        assertEquals(generated, store.revision())
        assertEquals(4, store.materialLoads)
        store.remove()
        assertEquals(generated, store.revision())
        assertEquals(5, store.materialLoads)
        assertTrue(store.ensureDefault(setOf("192.168.1.3")))
        assertNotEquals(generated, store.revision())
        assertEquals(6, store.materialLoads)
        assertFalse(store.ensureDefault(addresses))
        assertEquals(6, store.materialLoads)
    }

    @Test fun removalWithoutGeneratedMaterialCannotReturnCachedCertificate() {
        val store = TlsStore(temporary.newFolder())
        store.importFrom(SyntheticIdentity().pkcs12(charArrayOf()), charArrayOf())
        assertNotNull(store.revision())
        store.remove()
        assertNull(store.revision())
        assertNull(store.sslContext())
        assertEquals(MaterialStatus.Missing, store.status())
        store.importFrom(SyntheticIdentity().pkcs12(charArrayOf()), charArrayOf())
        assertNotNull(store.revision())
        assertEquals(2, store.materialLoads)
    }

    @Test fun aNewGenerationReloadsEvenWithIdenticalFileSizesAndTimes() {
        val root = temporary.newFolder()
        val writer = TlsStore(root)
        val password = "test password".toCharArray()
        val bytes = SyntheticIdentity().pkcs12(password)
        writer.importFrom(bytes, password)
        val store = TlsStore(root)
        val revision = store.revision()
        val original = File(root, "tls").canonicalFile
        val times = listOf("certificate.p12", "password").associateWith { File(original, it).lastModified() }
        writer.importFrom(bytes, password)
        val replacement = File(root, "tls").canonicalFile
        assertNotEquals(original, replacement)
        times.forEach { (name, time) -> Files.setLastModifiedTime(File(replacement, name).toPath(), FileTime.fromMillis(time)) }
        assertEquals(revision, store.revision())
        assertEquals(2, store.materialLoads)
        assertEquals(revision, store.revision())
        assertEquals(2, store.materialLoads)
    }

    @Test fun eitherFilesModificationTimeReloadsMaterial() {
        val root = temporary.newFolder()
        val store = TlsStore(root)
        store.importFrom(SyntheticIdentity().pkcs12(charArrayOf()), charArrayOf())
        val revision = store.revision()
        for ((index, name) in listOf("certificate.p12", "password").withIndex()) {
            val file = File(root, "tls/$name")
            Files.setLastModifiedTime(file.toPath(), FileTime.fromMillis(file.lastModified() + 2_000))
            assertEquals(revision, store.revision())
            assertEquals(index + 2, store.materialLoads)
        }
    }

    @Test fun changedFileLengthsInvalidateCachedMaterial() {
        for (name in listOf("certificate.p12", "password")) {
            val root = temporary.newFolder()
            val store = TlsStore(root)
            store.importFrom(SyntheticIdentity().pkcs12(charArrayOf()), charArrayOf())
            assertNotNull(store.revision())
            val file = File(root, "tls/$name")
            val time = Files.getLastModifiedTime(file.toPath())
            file.writeBytes(byteArrayOf(1))
            Files.setLastModifiedTime(file.toPath(), time)
            assertThrows(Exception::class.java) { store.revision() }
            assertEquals(2, store.materialLoads)
            assertTrue(store.status() is MaterialStatus.Invalid)
        }
    }

    @Test fun failedLoadsAreRetriedAndCanRecoverWithoutAnotherIdentityChange() {
        val root = temporary.newFolder()
        val store = TlsStore(root)
        val password = "test password".toCharArray()
        store.importFrom(SyntheticIdentity().pkcs12(password), password)
        val revision = store.revision()
        val file = File(root, "tls/password")
        val time = FileTime.fromMillis(file.lastModified() + 2_000)
        file.writeText("x".repeat(password.size))
        Files.setLastModifiedTime(file.toPath(), time)
        repeat(2) { assertThrows(Exception::class.java) { store.revision() } }
        assertEquals(3, store.materialLoads)
        file.writeText(String(password))
        Files.setLastModifiedTime(file.toPath(), time)
        assertEquals(revision, store.revision())
        assertEquals(4, store.materialLoads)
        assertEquals(revision, store.revision())
        assertEquals(4, store.materialLoads)
    }

    @Test fun cachedValidityUsesInjectedClockAtBothDateBoundaries() {
        for (expiry in listOf(false, true)) {
            val root = temporary.newFolder()
            var time = System.currentTimeMillis()
            val store = TlsStore(root, { time })
            assertTrue(store.ensureDefault(addresses))
            val certificate = certificate(File(root, "tls-generated"))
            val revision = store.revision()
            time = if (expiry) certificate.notAfter.time else certificate.notBefore.time
            assertFalse(store.ensureDefault(addresses))
            assertEquals(1, store.materialLoads)
            time += if (expiry) 1L else -1L
            assertEquals(revision, store.revision())
            assertEquals(MaterialStatus.Installed, store.status())
            assertNotNull(store.sslContext())
            assertEquals(1, store.materialLoads)
            assertTrue(store.ensureDefault(addresses))
            assertNotEquals(revision, store.revision())
            assertEquals(2, store.materialLoads)
        }
    }

    private fun certificate(directory: File): X509Certificate {
        val password = File(directory, "password").readText().toCharArray()
        try {
            val keys = KeyStore.getInstance("PKCS12")
            File(directory, "certificate.p12").inputStream().use { keys.load(it, password) }
            return keys.getCertificate("relay") as X509Certificate
        } finally { password.fill('\u0000') }
    }
}
