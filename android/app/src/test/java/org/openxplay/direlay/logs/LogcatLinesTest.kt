// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-23
package org.openxplay.direlay.logs

import org.junit.Assert.*
import org.junit.Test

class LogcatLinesTest {
    @Test fun shortLinesStayIdenticalIncludingTheBoundary() {
        for (text in listOf("", "hello", "x".repeat(3500))) assertEquals(listOf(text), LogcatLines.split(text))
    }
    @Test fun longUtf8LinesKeepEveryCodePointAndPartNumber() {
        val original = "prefix " + "汽车🚗".repeat(4000)
        val lines = LogcatLines.split(original)
        assertTrue(lines.size > 2)
        lines.forEachIndexed { i, line ->
            assertTrue(line.toByteArray(Charsets.UTF_8).size <= 3500)
            assertTrue(line.endsWith(" part=${i + 1}/${lines.size}"))
            assertFalse(line.contains('\ufffd'))
        }
        assertEquals(original, lines.joinToString("") { it.substringBeforeLast(" part=") })
    }
}
