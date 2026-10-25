// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-25
package org.openxplay.direlay.update

sealed interface ReleaseNoteBlock {
    val text: String
    data class Heading(override val text: String) : ReleaseNoteBlock
    data class Bullet(override val text: String) : ReleaseNoteBlock
    data class Paragraph(override val text: String) : ReleaseNoteBlock
}
data class LocalizedReleaseNotes(val summary: String, val blocks: List<ReleaseNoteBlock>)

object ReleaseNotes {
    fun localized(body: String, chinese: Boolean): LocalizedReleaseNotes {
        val lines = body.lineSequence().takeWhile { it != "---" }.toList()
        val summary = lines.firstOrNull().orEmpty()
        val separator = summary.lastIndexOf(" · ")
        val bilingual = separator >= 0 && han(summary.substring(separator + 3))
        val heading = lines.indexOfFirst { isHeading(it.trim()) && han(it) }
        val localizedSummary = if (bilingual) {
            if (chinese) summary.substring(separator + 3) else summary.substring(0, separator)
        } else summary
        val notes = if (heading < 0) lines else if (chinese) lines.drop(heading) else lines.drop(1).take((heading - 1).coerceAtLeast(0))
        return LocalizedReleaseNotes(clean(localizedSummary), blocks(notes))
    }

    private fun blocks(lines: List<String>): List<ReleaseNoteBlock> = buildList {
        val paragraph = mutableListOf<String>()
        fun flush() { if (paragraph.isNotEmpty()) { add(ReleaseNoteBlock.Paragraph(clean(paragraph.joinToString(" ")))); paragraph.clear() } }
        lines.forEach { raw ->
            val line = raw.trim()
            when {
                line.isEmpty() -> flush()
                isHeading(line) -> { flush(); add(ReleaseNoteBlock.Heading(clean(line))) }
                line.startsWith("- ") -> { flush(); add(ReleaseNoteBlock.Bullet(clean(line.drop(2)))) }
                else -> paragraph += line
            }
        }
        flush()
    }
    private fun isHeading(line: String) = line.length > 4 && line.startsWith("**") && line.endsWith("**")
    private fun han(value: String) = value.codePoints().anyMatch { Character.UnicodeScript.of(it) == Character.UnicodeScript.HAN }
    private fun clean(value: String) = value.replace("**", "").replace("`", "")
}
