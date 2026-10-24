// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-22
package org.openxplay.direlay.ui

import android.app.Application
import android.content.res.Configuration
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import org.openxplay.direlay.R
import org.openxplay.direlay.logs.LogRange
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config
import java.util.Locale

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [33], qualifiers = "en", application = Application::class)
class LogsPresentationTest {
    @Test fun logsMenuRowFollowsConfigurationFilesAndHasItsOwnTitleAndRoute() {
        assertEquals(listOf("settings/configFiles", "settings/logs"), settingsFilePages.map { it.route })
        val page = settingsFilePages.last()
        assertEquals(R.string.logs_title, page.title); assertEquals(R.string.logs_summary, page.summary)
        assertEquals(R.string.logs_title, settingsPageTitle(page.route))
        assertEquals(listOf("home", "settings", "settings/logs"), RelayBackStack().open(page.route).entries)
        assertEquals(listOf(R.string.logs_10_minutes, R.string.logs_30_minutes, R.string.logs_24_hours), LogRange.entries.map(::logRangeTitle))
    }
    @Test fun dialogsAndAcknowledgementsHaveEnglishAndSimplifiedChineseCopy() {
        val context = RuntimeEnvironment.getApplication()
        val chinese = context.createConfigurationContext(Configuration(context.resources.configuration).apply { setLocale(Locale.SIMPLIFIED_CHINESE) })
        val labels = mapOf(R.string.logs_description to "中继会在这部手机上保留最多 24 小时的日志。技术细节会保留；密码、密钥、电话号码、姓名、消息和精确位置会在存储前被替换。",
            R.string.logs_title to "日志", R.string.logs_10_minutes to "过去 10 分钟", R.string.logs_30_minutes to "过去 30 分钟",
            R.string.logs_24_hours to "过去 24 小时", R.string.logs_upload to "上传并显示二维码", R.string.logs_compressing to "正在压缩日志…",
            R.string.logs_copy to "复制链接", R.string.logs_empty to "这段时间没有记录到日志。", R.string.logs_no_internet to "没有可用的互联网连接。")
        labels.forEach { (id, text) -> assertEquals(text, chinese.getString(id)); assertNotEquals(text, context.getString(id)) }
        assertEquals("The relay keeps up to 24 hours of its logs on this phone. Technical details are kept; passwords, keys, phone numbers, names, messages and precise location are replaced before anything is stored.", context.getString(R.string.logs_description))
        for (localized in listOf(context, chinese)) {
            assertTrue(localized.getString(R.string.logs_upload_hint).contains("logs.openxplay.org"))
            assertTrue(localized.getString(R.string.receiver_based_on_xcertplay_licensed_under_gpl_3_0_diplay).contains("ZXing"))
        }
    }
}
