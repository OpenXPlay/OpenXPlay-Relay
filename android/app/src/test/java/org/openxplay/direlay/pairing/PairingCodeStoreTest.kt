// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-14
package org.openxplay.direlay.pairing

import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder
import java.io.File
import java.nio.file.Files
import java.nio.file.attribute.PosixFilePermissions
import java.security.SecureRandom

class PairingCodeStoreTest {
    @get:Rule val temporary = TemporaryFolder()
    @Test fun firstStartGeneratesEightDigitsAndReloadKeepsThem() {
        val root = temporary.newFolder()
        val code = PairingCodeStore(root).code()
        assertTrue(code.matches(Regex("[0-9]{8}")))
        assertEquals(code, PairingCodeStore(root).code())
        assertEquals(PosixFilePermissions.fromString("rw-------"), Files.getPosixFilePermissions(File(root, "pairing/code").toPath()))
        assertEquals(PosixFilePermissions.fromString("rwx------"), Files.getPosixFilePermissions(File(root, "pairing").toPath()))
    }
    @Test fun leadingZeroesPersistAndRegenerationCannotKeepOldToken() {
        val values = java.util.ArrayDeque(listOf(123, 123, 99_999_999))
        val random = object : SecureRandom() { override fun nextInt(bound: Int): Int {
            assertEquals(100_000_000, bound)
            return values.removeFirst()
        } }
        val root = temporary.newFolder()
        val store = PairingCodeStore(root, random)
        assertEquals("00000123", store.code())
        assertEquals("99999999", store.regenerate())
        assertEquals("99999999", PairingCodeStore(root).code())
        assertEquals(1, root.listFiles().orEmpty().count { it.name.startsWith(".pairing-generation-") })
    }
    @Test fun corruptSavedCodeFailsRatherThanPublishingAnUnexpectedToken() {
        val root = temporary.newFolder()
        val store = PairingCodeStore(root)
        store.code()
        File(root, "pairing/code").writeText("abcdefgh")
        assertThrows(IllegalArgumentException::class.java) { store.code() }
        assertTrue(store.regenerate().matches(Regex("[0-9]{8}")))
    }
}
