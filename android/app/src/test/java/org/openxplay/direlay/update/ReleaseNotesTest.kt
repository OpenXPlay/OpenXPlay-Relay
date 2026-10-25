// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-25
package org.openxplay.direlay.update

import org.junit.Assert.*
import org.junit.Test

class ReleaseNotesTest {
    @Test fun languagesSummaryHeadingsAndBulletsAreSplitAndProvenanceIsRemoved() {
        val english = ReleaseNotes.localized(ReleaseFixtures.body, false)
        val chinese = ReleaseNotes.localized(ReleaseFixtures.body, true)
        assertEquals("Android app: faster pairing", english.summary)
        assertEquals("安卓应用：更快配对", chinese.summary)
        assertTrue(english.blocks.contains(ReleaseNoteBlock.Paragraph("Pairing improvements for Carlink.")))
        assertTrue(english.blocks.contains(ReleaseNoteBlock.Heading("New")))
        assertTrue(english.blocks.contains(ReleaseNoteBlock.Bullet("A beta option")))
        assertTrue(chinese.blocks.contains(ReleaseNoteBlock.Heading("新增")))
        assertTrue(chinese.blocks.contains(ReleaseNoteBlock.Bullet("测试版选项")))
        assertTrue(english.blocks.any { it.text == "[Guide](https://openxplay.org/en/)" })
        assertFalse(english.blocks.any { "新增" in it.text || "测试版" in it.text })
        assertFalse(chinese.blocks.any { "Pairing improvements" in it.text || "New" == it.text })
        for (notes in listOf(english, chinese)) assertFalse(notes.blocks.any { "private-build-provenance" in it.text || "SHA256SUMS" in it.text })
    }
    @Test fun withoutAHanHeadingTheWholeCutBodyIsUsedForBothLanguages() {
        val body = "Summary\n\n**New**\n- One\n\nA paragraph\nover two lines\n---\nCommit"
        val english = ReleaseNotes.localized(body, false)
        assertEquals(english, ReleaseNotes.localized(body, true))
        assertEquals(listOf(ReleaseNoteBlock.Paragraph("Summary"), ReleaseNoteBlock.Heading("New"), ReleaseNoteBlock.Bullet("One"),
            ReleaseNoteBlock.Paragraph("A paragraph over two lines")), english.blocks)
    }
    @Test fun summaryUsesTheLastBilingualSeparatorAndOnlyExactProvenanceLinesAreCut() {
        val body = "App · Android · 安卓摘要\r\n\r\n**Notes**\r\n- Keep --- inline\r\n --- \r\n**说明**\r\n- 保留\r\n---\r\nCommit"
        val english = ReleaseNotes.localized(body, false)
        assertEquals("App · Android", english.summary)
        assertEquals("安卓摘要", ReleaseNotes.localized(body, true).summary)
        assertTrue(english.blocks.any { it.text == "---" })
        assertTrue(english.blocks.any { it.text == "Keep --- inline" })
        assertTrue(ReleaseNotes.localized("", false).blocks.isEmpty())
        assertEquals("One · two", ReleaseNotes.localized("One · two", true).summary)
    }
}
