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
        assertEquals(R.string.logs_title, page.title)
        assertEquals(R.string.logs_title, settingsPageTitle(page.route))
        assertEquals(listOf("home", "settings", "settings/logs"), RelayBackStack().open(page.route).entries)
        assertEquals(listOf(R.string.logs_10_minutes, R.string.logs_30_minutes, R.string.logs_24_hours), LogRange.entries.map(::logRangeTitle))
    }
    @Test fun dialogsAndAcknowledgementsHaveEnglishAndSimplifiedChineseCopy() {
        val context = RuntimeEnvironment.getApplication()
        val chinese = context.createConfigurationContext(Configuration(context.resources.configuration).apply { setLocale(Locale.SIMPLIFIED_CHINESE) })
        val labels = mapOf(R.string.logs_privacy_local to "日志默认只保存在这部手机上，保留最近约 24 小时。只有你点击“上传并显示二维码”并确认同意后，日志才会上传。",
            R.string.logs_title to "日志", R.string.logs_10_minutes to "过去 10 分钟", R.string.logs_30_minutes to "过去 30 分钟",
            R.string.logs_24_hours to "过去 24 小时", R.string.logs_upload to "上传并显示二维码", R.string.logs_compressing to "正在压缩日志…",
            R.string.logs_copy to "复制链接", R.string.logs_empty to "这段时间没有记录到日志。", R.string.logs_no_internet to "没有可用的互联网连接。",
            R.string.logs_consent_title to "上传前请确认",
            R.string.logs_consent_destination to "日志将上传到 logs.openxplay.org，仅用于帮助排查连接失败等问题，不提供公开下载。",
            R.string.logs_consent_retention to "上传后 48 小时内可以查看回执；日志最多保存 72 小时，到期后自动从我们的服务器永久删除。这个 App 上传的日志会通过随机标识自动关联。",
            R.string.logs_consent_alternative to "如果介意，可以改为保存到手机，再私下发给作者。", R.string.logs_consent_agree to "同意并上传")
        labels.forEach { (id, text) -> assertEquals(text, chinese.getString(id)); assertNotEquals(text, context.getString(id)) }
        assertEquals("Logs stay on this phone by default, covering about the last 24 hours. They are uploaded only after you tap “Upload and show QR code” and agree.", context.getString(R.string.logs_privacy_local))
        for (localized in listOf(context, chinese)) {
            assertTrue(localized.getString(R.string.logs_consent_destination).contains("logs.openxplay.org"))
            assertTrue(localized.getString(R.string.receiver_based_on_xcertplay_licensed_under_gpl_3_0_diplay).contains("ZXing"))
        }
    }
}
