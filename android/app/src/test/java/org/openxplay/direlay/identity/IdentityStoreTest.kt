// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-14
package org.openxplay.direlay.identity

import com.shilapi.xcertplay.mfi.LocalMfiAuthenticationClient
import org.openxplay.direlay.SyntheticIdentity
import org.openxplay.direlay.storage.MaterialStatus
import java.io.File
import java.nio.file.Files
import java.nio.file.attribute.PosixFilePermissions
import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder

class IdentityStoreTest {
    @get:Rule val temporary = TemporaryFolder()

    @Test fun importReplaceAndRemove() {
        val root = temporary.newFolder()
        var selected = 0
        val store = IdentityStore(root) { selected++ }
        assertEquals(MaterialStatus.Missing, store.status())
        val first = SyntheticIdentity()
        store.importFrom(first.pair.private.encoded, first.pkcs7)
        val directory = File(root, "offline-mfi")
        assertEquals(MaterialStatus.Installed, store.status())
        assertArrayEquals(first.pkcs7, LocalMfiAuthenticationClient.load(directory).readCertificate())
        assertEquals(PosixFilePermissions.fromString("rwx------"), Files.getPosixFilePermissions(directory.toPath()))
        assertEquals(PosixFilePermissions.fromString("rw-------"), Files.getPosixFilePermissions(File(directory, "identity.pk8").toPath()))
        val second = SyntheticIdentity()
        store.importFrom(second.pair.private.encoded, second.pkcs7)
        assertArrayEquals(second.pkcs7, LocalMfiAuthenticationClient.load(directory).readCertificate())
        assertEquals(2, selected)
        store.remove()
        assertEquals(MaterialStatus.Missing, store.status())
        assertTrue(root.listFiles().orEmpty().isEmpty())
    }

    @Test fun injectedValidatorRejectsBeforeReplacingInstalledIdentity() {
        val root = temporary.newFolder()
        val identity = SyntheticIdentity()
        val key = identity.pair.private.encoded
        val certificate = identity.pkcs7
        var selected = 0
        val store = IdentityStore(root, validator = { staging ->
            require(File(staging, "identity.pk8").readBytes().contentEquals(key))
            require(File(staging, "certificate.p7b").readBytes().contentEquals(certificate))
        }) { selected++ }
        store.importFrom(key, certificate)
        assertThrows(IllegalArgumentException::class.java) { store.importFrom(key, byteArrayOf(1)) }
        assertArrayEquals(certificate, File(root, "offline-mfi/certificate.p7b").readBytes())
        assertEquals(MaterialStatus.Installed, store.status())
        assertEquals(1, selected)
    }

    @Test fun rejectedImportsPreserveExistingIdentity() {
        val root = temporary.newFolder()
        var selected = 0
        val store = IdentityStore(root) { selected++ }
        val identity = SyntheticIdentity()
        store.importFrom(identity.pair.private.encoded, identity.pkcs7)
        val original = File(root, "offline-mfi").canonicalPath
        for ((key, certificate) in listOf(
            byteArrayOf(1) to identity.pkcs7,
            identity.pair.private.encoded to byteArrayOf(1),
            SyntheticIdentity().pair.private.encoded to identity.pkcs7,
            ByteArray(IdentityStore.MAX_FILE_BYTES + 1) to identity.pkcs7,
            byteArrayOf() to identity.pkcs7,
        )) {
            assertThrows(Exception::class.java) { store.importFrom(key, certificate) }
            assertEquals(original, File(root, "offline-mfi").canonicalPath)
            assertEquals(MaterialStatus.Installed, store.status())
        }
        assertEquals(1, selected)
        assertEquals(2, root.listFiles()!!.size)
        File(root, "offline-mfi/certificate.p7b").writeBytes(byteArrayOf(1))
        assertTrue(store.status() is MaterialStatus.Invalid)
    }
}
