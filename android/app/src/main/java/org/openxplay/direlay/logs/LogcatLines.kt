// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-23
package org.openxplay.direlay.logs

/** Split by UTF-8 bytes, retaining code points and room for the part suffix. Redact before splitting. */
object LogcatLines {
    fun split(line: String, limit: Int = 3500): List<String> {
        require(limit >= 64)
        if (line.toByteArray(Charsets.UTF_8).size <= limit) return listOf(line)
        val chunks = mutableListOf<String>()
        val budget = limit - 40
        var start = 0
        var index = 0
        var bytes = 0
        while (index < line.length) {
            val point = line.codePointAt(index)
            val size = when { point <= 0x7f -> 1; point <= 0x7ff -> 2; point <= 0xffff -> 3; else -> 4 }
            if (bytes + size > budget) { chunks += line.substring(start, index); start = index; bytes = 0 }
            bytes += size; index += Character.charCount(point)
        }
        chunks += line.substring(start)
        return chunks.mapIndexed { i, chunk -> "$chunk part=${i + 1}/${chunks.size}" }
    }
}
