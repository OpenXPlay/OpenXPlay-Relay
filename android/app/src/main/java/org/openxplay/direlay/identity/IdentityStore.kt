// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-14
package org.openxplay.direlay.identity

import com.shilapi.xcertplay.mfi.LocalMfiAuthenticationClient
import org.openxplay.direlay.storage.MaterialStatus
import org.openxplay.direlay.storage.PrivateStore
import java.io.File

class IdentityStore(noBackupFilesDir: File,
    private val validator: (File) -> Unit = { LocalMfiAuthenticationClient.load(it); Unit },
    private val selectLocal: () -> Unit = {},
) {
    private val store = PrivateStore(noBackupFilesDir, LocalMfiAuthenticationClient.DIRECTORY)

    @Synchronized fun status(): MaterialStatus = store.status(validator)

    @Synchronized fun importFrom(identity: ByteArray, certificate: ByteArray) {
        require(identity.size in 1..MAX_FILE_BYTES && certificate.size in 1..MAX_FILE_BYTES) {
            "Each identity file must contain 1–16384 bytes"
        }
        store.replace { staging ->
            PrivateStore.write(staging, "identity.pk8", identity)
            PrivateStore.write(staging, "certificate.p7b", certificate)
            try { validator(staging) }
            catch (_: Exception) { throw IllegalArgumentException("Invalid identity: expected a matching P-256 PKCS8 key and accessory certificate") }
        }
        selectLocal()
    }

    @Synchronized fun remove() = store.remove()

    companion object { const val MAX_FILE_BYTES = 16 * 1024 }
}
