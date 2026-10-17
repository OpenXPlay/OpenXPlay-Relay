// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-14
package org.openxplay.direlay.clink

import org.openxplay.direlay.storage.PrivateStore
import org.openxplay.stream.clink.TrustStore
import org.openxplay.stream.clink.protocol.AuthIdentity
import java.io.File
import java.security.MessageDigest
import java.nio.file.Files
import java.nio.file.LinkOption.NOFOLLOW_LINKS

/** Copy-on-write generations share the same 0700/0600 convention as identity and TLS. */
class PrivateClinkTrustStore(root: File) : TrustStore {
    private val store = PrivateStore(root, "clink-trust")
    @Synchronized override fun loadIdentity(): AuthIdentity? {
        if (!Files.exists(store.directory.toPath(), NOFOLLOW_LINKS)) return null
        val key = File(store.directory, "identity.pk8")
        if (!key.exists()) {
            check(!File(store.directory, "identity.pub").exists()) { "Incomplete stored Carlink identity" }
            return null
        }
        require(key.length() in 1..16384)
        val public = File(store.directory, "identity.pub")
        require(public.length() == 64L) { "Invalid stored Carlink identity" }
        val encoded = key.readBytes()
        try { return AuthIdentity(encoded, public.readBytes()) }
        finally { encoded.fill(0) }
    }
    @Synchronized override fun saveIdentity(identity: AuthIdentity) = update {
        val encoded = identity.pkcs8
        try { PrivateStore.write(it, "identity.pk8", encoded) } finally { encoded.fill(0) }
        PrivateStore.write(it, "identity.pub", identity.publicKey)
    }
    @Synchronized override fun loadPeerKey(headUnitId: String): ByteArray? {
        val file = File(store.directory, peerName(headUnitId))
        if (!file.exists()) return null
        require(file.length() == 64L) { "Invalid stored Carlink peer key" }
        return file.readBytes()
    }
    @Synchronized override fun savePeerKey(headUnitId: String, publicKey: ByteArray) {
        require(publicKey.size == 64)
        val name = peerName(headUnitId)
        update(setOf(name)) { PrivateStore.write(it, name, publicKey.copyOf()) }
    }
    @Synchronized override fun forgetPeer(headUnitId: String) = update(setOf(peerName(headUnitId))) {}
    private fun update(excluded: Set<String> = setOf("identity.pk8", "identity.pub"), write: (File) -> Unit) {
        store.replace { staging ->
            store.directory.listFiles().orEmpty().filter { it.name !in excluded }.forEach { file ->
                require(file.isFile && file.length() in 1..16384)
                val bytes = file.readBytes()
                try { PrivateStore.write(staging, file.name, bytes) } finally { bytes.fill(0) }
            }
            write(staging)
        }
    }
    private fun peerName(id: String): String = "peer-" + MessageDigest.getInstance("SHA-256")
        .digest(id.toByteArray(Charsets.UTF_8)).joinToString("") { "%02x".format(it.toInt() and 255) }
}
