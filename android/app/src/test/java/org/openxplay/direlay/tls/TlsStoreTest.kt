// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-14
package org.openxplay.direlay.tls

import org.openxplay.direlay.SyntheticIdentity
import org.openxplay.direlay.storage.MaterialStatus
import java.io.File
import java.security.KeyStore
import java.nio.file.Files
import java.nio.file.attribute.PosixFilePermissions
import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder

class TlsStoreTest {
    @get:Rule val temporary = TemporaryFolder()

    @Test fun validImportReloadReplaceAndRemove() {
        val root = temporary.newFolder()
        var changes = 0
        val store = TlsStore(root) { changes++ }
        val password = " secret with spaces ".toCharArray()
        assertNull(store.sslContext())
        val data = SyntheticIdentity().pkcs12(password)
        store.importFrom(data, password)
        assertEquals(MaterialStatus.Installed, store.status())
        assertNotNull(TlsStore(root).sslContext())
        assertEquals(PosixFilePermissions.fromString("rwx------"), Files.getPosixFilePermissions(File(root, "tls").toPath()))
        assertEquals(PosixFilePermissions.fromString("rw-------"), Files.getPosixFilePermissions(File(root, "tls/password").toPath()))
        store.importFrom(SyntheticIdentity().pkcs12(charArrayOf()), charArrayOf())
        assertNotNull(store.sslContext())
        // The replaced key material is deleted; only the active generation remains.
        assertEquals(1, root.listFiles().orEmpty().count { it.name.startsWith(".tls-generation-") })
        store.remove()
        assertNull(store.sslContext())
        assertEquals(MaterialStatus.Missing, store.status())
        assertEquals(3, changes)
        assertTrue(root.listFiles().orEmpty().isEmpty())
    }

    @Test fun wrongPasswordCorruptEmptyAndOversizedFilesPreserveExistingTls() {
        val root = temporary.newFolder()
        var changes = 0
        val store = TlsStore(root) { changes++ }
        val password = "test password".toCharArray()
        val data = SyntheticIdentity().pkcs12(password)
        store.importFrom(data, password)
        val original = File(root, "tls").canonicalPath
        val empty = java.io.ByteArrayOutputStream().also {
            KeyStore.getInstance("PKCS12").apply { load(null, password); store(it, password) }
        }.toByteArray()
        for ((bytes, secret) in listOf(
            data to "wrong".toCharArray(), byteArrayOf(1, 2, 3) to password,
            empty to password, ByteArray(TlsStore.MAX_FILE_BYTES + 1) to password,
        )) {
            assertThrows(Exception::class.java) { store.importFrom(bytes, secret) }
            assertEquals(original, File(root, "tls").canonicalPath)
            assertNotNull(store.sslContext())
        }
        assertEquals(1, changes)
        assertEquals(2, root.listFiles()!!.size)
        File(root, "tls/certificate.p12").writeBytes(byteArrayOf(1))
        assertTrue(store.status() is MaterialStatus.Invalid)
        assertThrows(Exception::class.java) { store.sslContext() }
    }

    @Test fun webPortsFollowTlsSelection() {
        assertEquals(8080, TlsStore.port(false))
        assertEquals(8443, TlsStore.port(true))
    }
}
