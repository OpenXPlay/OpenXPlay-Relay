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
    @Test fun recommendationAndGenericActionMatchBothLocales() {
        val labels = mapOf(
            R.string.car_zeekr_trusted_https_hint to ("ZEEKR owners: set up Trusted HTTPS for the car's browser." to "极氪车主建议设置可信 HTTPS。"),
            R.string.car_open_trusted_https to ("Set up Trusted HTTPS" to "设置可信 HTTPS"))
        val app = RuntimeEnvironment.getApplication()
        for ((tag, chinese) in listOf("en" to false, "zh-CN" to true)) {
            val config = Configuration(app.resources.configuration).apply { setLocale(Locale.forLanguageTag(tag)) }
            val context = app.createConfigurationContext(config)
            labels.forEach { (id, texts) -> assertEquals(if (chinese) texts.second else texts.first, context.getString(id)) }
        }
    }
}
