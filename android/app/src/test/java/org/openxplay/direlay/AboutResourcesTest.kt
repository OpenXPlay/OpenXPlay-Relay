// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-14
package org.openxplay.direlay

import android.app.Application
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config
import java.util.Locale

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [33], qualifiers = "en", application = Application::class)
class AboutResourcesTest {
    @Test fun installedPackageAndAllLocaleNamesUseRelayBrand() {
        val context = RuntimeEnvironment.getApplication()
        assertEquals("org.openxplay.relay", context.packageName)
        for (tag in listOf("en", "ar", "es", "ja", "ru", "uk", "zh-CN", "zh-TW")) {
            val configuration = android.content.res.Configuration(context.resources.configuration).apply {
                setLocale(Locale.forLanguageTag(tag))
            }
            val localized = context.createConfigurationContext(configuration)
            assertEquals(tag, "OpenXPlay Relay", localized.getString(R.string.direlay_app_name))
            assertEquals(tag, "OpenXPlay Relay", localized.getString(R.string.app_name))
            assertEquals(tag, "OpenXPlay", localized.getString(R.string.relay_brand_first))
            assertEquals(tag, "Relay", localized.getString(R.string.relay_brand_second))
        }
    }

    @Test fun aboutContainsAcknowledgementsProjectLinksAndTrademarkStatement() {
        val context = RuntimeEnvironment.getApplication()
        val copy = context.getString(R.string.direlay_acknowledgement_text)
        for (name in listOf("DiPlay", "xcertplay", "GPL-3.0", "authors", "contributors")) {
            assertTrue(copy.contains(name))
        }
        assertEquals("About", context.getString(R.string.direlay_about))
        assertEquals("Powered by OpenXPlay.", context.getString(R.string.direlay_attribution))
        assertEquals("https://github.com/OpenXPlay/OpenXPlay-DiRelay", context.getString(R.string.direlay_project_url))
        assertEquals("https://github.com/shihabal3amri/DiPlay", context.getString(R.string.direlay_diplay_url))
        assertEquals("https://github.com/shilapi/xcertplay", context.getString(R.string.direlay_xcertplay_url))
        val trademark = context.getString(R.string.direlay_trademark)
        assertTrue(trademark.contains("CarPlay is a trademark of Apple Inc."))
        assertTrue(trademark.contains("not affiliated with or endorsed by Apple, DiPlay or xcertplay"))
        assertEquals("Built on DiPlay and xcertplay", context.getString(R.string.direlay_built_on))
    }

    @Test fun allLocalesDescribeTheProjectAndDerivedLicences() {
        val context = RuntimeEnvironment.getApplication()
        for (tag in listOf("en", "ar", "es", "ja", "ru", "uk", "zh-CN", "zh-TW")) {
            val configuration = android.content.res.Configuration(context.resources.configuration).apply {
                setLocale(Locale.forLanguageTag(tag))
            }
            val localized = context.createConfigurationContext(configuration)
            val text = localized.getString(R.string.about_licence)
            assertTrue("$tag lacks the project licence", text.startsWith("AGPL-3.0"))
            assertTrue("$tag lacks the derived licence", text.contains("DiPlay") && text.endsWith("GPL-3.0"))
        }
    }

    @Test fun allReceiverLocalesKeepTheAcknowledgementAndDependencyList() {
        val context = RuntimeEnvironment.getApplication()
        for (tag in listOf("en", "ar", "es", "ja", "ru", "uk", "zh-CN", "zh-TW")) {
            val configuration = android.content.res.Configuration(context.resources.configuration)
            configuration.setLocale(Locale.forLanguageTag(tag))
            val localized = context.createConfigurationContext(configuration)
            val text = localized.getString(com.shilapi.xcertplay.host.R.string.receiver_based_on_xcertplay_licensed_under_gpl_3_0_diplay)
            for (name in listOf("OpenXPlay Relay", "DiPlay", "xcertplay", "GPL-3.0", "AndroidX", "Bouncy Castle", "JmDNS", "SLF4J", "Apple Inc.")) {
                assertTrue("$tag lacks $name", text.contains(name))
            }
        }
    }

    @Test fun radioAndDeveloperCopyIsAvailableInEnglishAndSimplifiedChinese() {
        val context = RuntimeEnvironment.getApplication()
        val keys = listOf(R.string.option_browser_hint, R.string.option_carlink_hint,
            R.string.option_wireless_hint, R.string.option_usb_hint, R.string.option_direct_hint,
            R.string.option_hotspot_hint, R.string.option_wifi_hint, R.string.dev_sample_vehicle,
            R.string.dev_hotspot_security, R.string.dev_security_mixed, R.string.dev_usb_i2c,
            R.string.dev_linux_i2c, R.string.dev_disabled, R.string.dev_unavailable,
            R.string.direlay_display_profile, R.string.direlay_microphone_source)
        val chinese = android.content.res.Configuration(context.resources.configuration).apply {
            setLocale(Locale.SIMPLIFIED_CHINESE)
        }
        val localized = context.createConfigurationContext(chinese)
        keys.forEach { key ->
            assertTrue(context.getString(key).isNotBlank())
            assertTrue(localized.getString(key).isNotBlank())
            assertNotEquals(context.getString(key), localized.getString(key))
        }
        assertEquals("Off", context.getString(R.string.dev_disabled))
        assertEquals("已关闭", localized.getString(R.string.dev_disabled))
    }

    @Test fun networkAndPairingCopyExistsInAllEightLocales() {
        val context = RuntimeEnvironment.getApplication()
        val keys = listOf(R.string.direlay_car_hotspot, R.string.direlay_existing_wifi,
            R.string.ui_wifi_direct_recommended, R.string.option_direct_hint, R.string.option_hotspot_hint,
            R.string.option_wifi_hint, R.string.direlay_car_hotspot_hint, R.string.direlay_existing_wifi_hint,
            R.string.direlay_network_ssid, R.string.direlay_network_passphrase, R.string.settings_hotspot_name,
            R.string.settings_hotspot_password, R.string.settings_open_hotspot, R.string.ui_open_wifi,
            R.string.ui_automatic_paused, R.string.settings_require_code, R.string.settings_require_code_hint,
            R.string.network_iphone, R.string.network_this_phone, R.string.network_car, R.string.network_direct,
            R.string.network_wifi, R.string.network_hotspot, R.string.network_direct_description,
            R.string.network_hotspot_description, R.string.network_wifi_description,
            R.string.settings_hotspot_name_error, R.string.settings_hotspot_password_error)
        for (tag in listOf("en", "ar", "es", "ja", "ru", "uk", "zh-CN", "zh-TW")) {
            val configuration = android.content.res.Configuration(context.resources.configuration).apply {
                setLocale(Locale.forLanguageTag(tag))
            }
            val localized = context.createConfigurationContext(configuration)
            keys.forEach { assertTrue("$tag has empty copy for $it", localized.getString(it).isNotBlank()) }
            if (tag != "en") for (key in listOf(R.string.ui_automatic_paused, R.string.settings_require_code,
                R.string.option_hotspot_hint, R.string.option_wifi_hint, R.string.network_this_phone))
                assertNotEquals("$tag falls back to English", context.getString(key), localized.getString(key))
        }
    }
}
