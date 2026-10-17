// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-14
package org.openxplay.direlay.clink

import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder
import org.openxplay.stream.clink.protocol.AuthIdentity
import java.io.File
import java.nio.file.Files
import java.nio.file.attribute.PosixFilePermissions

class PrivateClinkTrustStoreTest {
    @get:Rule val temporary = TemporaryFolder()
    @Test fun identityPeersReloadReplaceAndForgetWithoutLosingOtherMaterial() {
        val root = temporary.newFolder()
        val store = PrivateClinkTrustStore(root)
        assertNull(store.loadIdentity()); assertNull(store.loadPeerKey("car"))
        val identity = AuthIdentity.generate()
        store.saveIdentity(identity)
        val peer = AuthIdentity.generate().publicKey
        store.savePeerKey("../../car", peer)
        peer.fill(0)
        val reloaded = PrivateClinkTrustStore(root)
        assertArrayEquals(identity.pkcs8, reloaded.loadIdentity()!!.pkcs8)
        val retained = reloaded.loadPeerKey("../../car")!!
        assertFalse(retained.all { it == 0.toByte() })
        retained.fill(0)
        assertFalse(reloaded.loadPeerKey("../../car")!!.all { it == 0.toByte() })
        val second = AuthIdentity.generate()
        store.saveIdentity(second)
        assertNotNull(store.loadPeerKey("../../car"))
        store.savePeerKey("other", second.publicKey)
        store.forgetPeer("../../car")
        assertNull(store.loadPeerKey("../../car")); assertNotNull(store.loadPeerKey("other"))
        assertArrayEquals(second.pkcs8, store.loadIdentity()!!.pkcs8)
        assertEquals(1, root.listFiles()!!.count { it.name.startsWith(".clink-trust-generation-") })
        val directory = File(root, "clink-trust")
        assertEquals(PosixFilePermissions.fromString("rwx------"), Files.getPosixFilePermissions(directory.toPath()))
        directory.listFiles()!!.forEach {
            assertEquals(PosixFilePermissions.fromString("rw-------"), Files.getPosixFilePermissions(it.toPath()))
        }
    }
    @Test fun invalidPeerWritePreservesIdentityAndCorruptIdentityFailsClosed() {
        val root = temporary.newFolder()
        val store = PrivateClinkTrustStore(root)
        val identity = AuthIdentity.generate()
        store.saveIdentity(identity)
        try { store.savePeerKey("car", ByteArray(63)); fail("Expected rejection") }
        catch (_: IllegalArgumentException) { }
        assertArrayEquals(identity.pkcs8, store.loadIdentity()!!.pkcs8)
        File(root, "clink-trust/identity.pk8").writeBytes(byteArrayOf(1))
        try { store.loadIdentity(); fail("Expected corrupt identity rejection") }
        catch (_: Exception) { }
    }
}
