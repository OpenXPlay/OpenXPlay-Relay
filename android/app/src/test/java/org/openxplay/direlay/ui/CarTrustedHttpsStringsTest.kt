// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-27
package org.openxplay.direlay.ui

import android.app.Application
import android.content.res.Configuration
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import org.openxplay.direlay.R
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config
import java.util.Locale

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [33], qualifiers = "en", application = Application::class)
class CarTrustedHttpsStringsTest {
    @Test fun requirementNoticeAndActionMatchBothLocales() {
        val labels = mapOf(
            R.string.car_trusted_https_hint to ("This car's browser needs Trusted HTTPS with a public certificate." to "此车型的浏览器必须配置公网证书（可信 HTTPS）。"),
            R.string.stage_notice_https to ("Car browser needs a public certificate" to "车机浏览器需要公网证书"),
            R.string.public_name_required to ("Your car's browser opens the page only with a public certificate. Set up a public name to get one." to
                "你的车机浏览器必须使用公网证书才能打开页面。设置公网域名即可获取证书。"),
            R.string.car_open_trusted_https to ("Set up Trusted HTTPS" to "设置可信 HTTPS"))
        val app = RuntimeEnvironment.getApplication()
        for ((tag, chinese) in listOf("en" to false, "zh-CN" to true)) {
            val config = Configuration(app.resources.configuration).apply { setLocale(Locale.forLanguageTag(tag)) }
            val context = app.createConfigurationContext(config)
            labels.forEach { (id, texts) -> assertEquals(if (chinese) texts.second else texts.first, context.getString(id)) }
        }
    }
}
