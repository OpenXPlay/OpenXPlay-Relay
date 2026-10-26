// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-22
package org.openxplay.direlay.logs

import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder
import java.io.File
import java.io.IOException

class LogTokensTest {
    @get:Rule val temporary = TemporaryFolder()
    @Test fun installKeySurvivesReloadAndLivesOutsideLogSegments() {
        val root = temporary.newFolder()
        val file = File(root, "log-token.key")
        val first = LogTokens.loadKey(file)
        assertEquals(32, first.size)
        assertArrayEquals(first, LogTokens.loadKey(file))
        assertEquals(LogTokens(first).token("private"), LogTokens(LogTokens.loadKey(file)).token("private"))
        assertFalse(File(root, "logs/log-token.key").exists())
        assertEquals(listOf("log-token.key"), root.list()!!.toList())
        assertNotEquals(LogTokens(first).token("private"), LogTokens(LogTokens.loadKey(File(temporary.newFolder(), "log-token.key"))).token("private"))
    }
    @Test fun invalidExistingKeyIsNeverSilentlyReplaced() {
        val file = temporary.newFile("log-token.key").apply { writeBytes(byteArrayOf(1, 2)) }
        try { LogTokens.loadKey(file); fail("Expected an invalid key failure") } catch (_: IOException) { }
        assertArrayEquals(byteArrayOf(1, 2), file.readBytes())
    }
    @Test fun keyIsDefensivelyCopiedAndTextIncludesOriginalLength() {
        val key = ByteArray(32) { it.toByte() }
        val tokens = LogTokens(key)
        val before = tokens.token("private")
        key.fill(0)
        assertEquals(before, tokens.token("private"))
        assertTrue(tokens.token("Secret song", "text").matches(Regex("\\[text#[0-9a-f]{6} len=11]")))
        assertEquals("LogTokens([secret])", tokens.toString())
    }
}
