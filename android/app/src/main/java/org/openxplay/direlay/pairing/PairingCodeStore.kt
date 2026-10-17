// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-14
package org.openxplay.direlay.pairing

import org.openxplay.direlay.storage.PrivateStore
import java.io.File
import java.security.SecureRandom

/** A no-backup, atomically replaced token. Leading zeroes are significant. */
class PairingCodeStore(root: File, private val random: SecureRandom = SecureRandom()) {
    private val store = PrivateStore(root, "pairing")
    @Synchronized fun code(): String {
        val file = File(store.directory, "code")
        if (file.exists()) {
            require(file.length() == 8L) { "Invalid stored pairing code" }
            return file.readText(Charsets.US_ASCII).also { require(it.matches(Regex("[0-9]{8}"))) }
        }
        return regenerate()
    }
    @Synchronized fun regenerate(): String {
        val old = File(store.directory, "code").takeIf { it.isFile }?.readText()
        var code: String
        do { code = random.nextInt(100_000_000).toString().padStart(8, '0') } while (code == old)
        store.replace { PrivateStore.write(it, "code", code.toByteArray(Charsets.US_ASCII)) }
        return code
    }
}
