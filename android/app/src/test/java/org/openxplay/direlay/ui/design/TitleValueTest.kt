// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-24
package org.openxplay.direlay.ui.design

import org.junit.Assert.*
import org.junit.Test

class TitleValueTest {
    @Test fun shortValueFitsInline() {
        assertTrue(valueFitsInline(80, 40, 12, 160))
    }
    @Test fun exactWidthFitsInline() {
        assertTrue(valueFitsInline(80, 40, 12, 132))
    }
    @Test fun onePixelOverStacksValue() {
        assertFalse(valueFitsInline(80, 40, 12, 131))
    }
    @Test fun zeroWidthValueFitsInline() {
        assertTrue(valueFitsInline(80, 0, 12, 92))
        assertFalse(valueFitsInline(80, 0, 12, 91))
    }
    @Test fun chineseSummarySegmentsKeepCompleteItems() {
        assertEquals(listOf("自动启动：开启 ·", "开机启动：关闭"), summarySegments("自动启动：开启 · 开机启动：关闭"))
        assertEquals(listOf("管理页面：开启 ·", "可信 HTTPS：开启"), summarySegments("管理页面：开启 · 可信 HTTPS：开启"))
    }
    @Test fun englishSummarySegmentsKeepCompleteItems() {
        assertEquals(listOf("Auto-start: On ·", "Start on boot: Off"), summarySegments("Auto-start: On · Start on boot: Off"))
    }
    @Test fun foldedStepSummarySegmentsKeepCompleteNames() {
        assertEquals(listOf("已完成 5 项 ·", "连接方案 ·", "认证身份 ·", "系统权限 ·", "蓝牙 ·", "Wi-Fi"),
            summarySegments("已完成 5 项 · 连接方案 · 认证身份 · 系统权限 · 蓝牙 · Wi-Fi"))
    }
    @Test fun singleSummarySegmentStaysUnchanged() {
        assertEquals(listOf("Wi-Fi enabled"), summarySegments("Wi-Fi enabled"))
    }
    @Test fun summarySegmentsKeepSeparatorsExceptOnLast() {
        assertEquals(listOf("A ·", "B ·", "C"), summarySegments("A · B · C"))
    }
    @Test fun summarySegmentsDropBlankItems() {
        assertEquals(listOf("A ·", "B ·", "C"), summarySegments(" · A ·  · B ·    · C · "))
        assertEquals(emptyList<String>(), summarySegments(""))
        assertEquals(emptyList<String>(), summarySegments("   "))
        assertEquals(emptyList<String>(), summarySegments(" ·  · "))
    }
}
