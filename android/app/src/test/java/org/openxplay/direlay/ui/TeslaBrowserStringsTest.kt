// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-22
package org.openxplay.direlay.ui

import android.app.Application
import android.content.Context
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
class TeslaBrowserStringsTest {
    private fun localized(tag: String): Context {
        val context = RuntimeEnvironment.getApplication()
        val configuration = Configuration(context.resources.configuration).apply { setLocale(Locale.forLanguageTag(tag)) }
        return context.createConfigurationContext(configuration)
    }
    private val labels = mapOf(
        R.string.tesla_title to ("Tesla browser" to "特斯拉浏览器适配"),
        R.string.tesla_intro to ("Tesla’s browser does not open local-network addresses, so the relay page needs another address. This phone adds one through a local VPN (by default 169.254.88.1); nothing leaves the phone." to "特斯拉的浏览器不能打开局域网地址，所以中继页面需要另一个地址。这台手机会通过本地 VPN 加上这个地址（默认 169.254.88.1），数据不会离开手机。"),
        R.string.tesla_off to ("Off" to "关闭"),
        R.string.tesla_virtual_name to ("Virtual address with a trusted name (like TeslaMirror)" to "虚拟地址 + 可信域名（TeslaMirror 方式）"),
        R.string.tesla_virtual_name_hint to ("Needs Trusted HTTPS. Your trusted name points to a virtual address on this phone, so the Tesla opens https://your-name:8443 without a certificate warning. The Tesla needs internet through this phone’s hotspot to look up the name." to "需要先设置可信 HTTPS。你的可信域名会指向这台手机上的虚拟地址，特斯拉打开 https://你的域名:8443 时不会出现证书警告。特斯拉要能通过这台手机的热点上网，才能查到这个域名。"),
        R.string.tesla_public_address to ("Direct address (like TeslaAndroid)" to "直接地址（TeslaAndroid 方式）"),
        R.string.tesla_public_address_hint to ("No account needed. Open https://%1\$s:8443 in the Tesla and continue past the certificate warning. This works only if the Tesla browser lets you continue." to "不需要账号。在特斯拉中打开 https://%1\$s:8443，并在证书警告页选择继续访问。只有特斯拉浏览器允许继续访问时才能使用。"),
        R.string.tesla_address to ("Address on this phone" to "本机虚拟地址"),
        R.string.tesla_address_invalid to ("Enter an IPv4 address outside 10.x, 172.16–31.x, 192.168.x and the other reserved ranges." to "请输入不在 10.x、172.16–31.x、192.168.x 和其他保留网段内的 IPv4 地址。"),
        R.string.tesla_address_android13 to ("Android 13 and later drop connections to this address from the hotspot. Use an address from 169.254.1.0 to 169.254.254.255." to "Android 13 及更高版本会丢弃热点上的设备发往这个地址的连接，请使用 169.254.1.0 到 169.254.254.255 之间的地址。"),
        R.string.tesla_requirements to ("Connect the Tesla to this phone’s hotspot (5 GHz if possible) or its Wi-Fi Direct network, and turn on Remain Connected in Drive in the Tesla’s Wi-Fi settings. Needs MCU2 or later." to "让特斯拉连接这台手机的热点（最好用 5 GHz）或 Wi-Fi 直连网络，并在特斯拉的 Wi-Fi 设置中打开“驾驶时保持连接”（Remain Connected in Drive）。需要 MCU2 或更新的车机。"),
        R.string.tesla_needs_browser to ("A Tesla shows CarPlay in its browser: choose Browser (or Browser and Carlink) in Settings → Output." to "特斯拉通过浏览器显示 CarPlay：请在「设置 → 输出」中选择浏览器（或浏览器和 Carlink）。"),
        R.string.tesla_open_output to ("Open Output" to "打开输出设置"),
        R.string.tesla_needs_wireless to ("Tesla adaptation needs wireless CarPlay: the USB connection already uses Android’s only VPN." to "特斯拉适配需要使用无线 CarPlay：USB 连接已经占用了 Android 唯一的 VPN。"),
        R.string.tesla_needs_phone_network to ("Tesla adaptation needs this phone’s hotspot or Wi-Fi Direct. A shared Wi-Fi router does not pass the address to this phone." to "特斯拉适配需要使用本机热点或 Wi-Fi 直连。共享 Wi-Fi 路由器不会把这个地址转给这台手机。"),
        R.string.tesla_needs_name to ("Set up Trusted HTTPS first." to "请先设置可信 HTTPS。"),
        R.string.tesla_open_name to ("Open Trusted HTTPS" to "打开可信 HTTPS"),
        R.string.tesla_address_failed to ("The Tesla address could not be added: %1\$s" to "无法添加特斯拉访问地址：%1\$s"),
        R.string.tesla_url to ("Tesla: %1\$s" to "特斯拉：%1\$s"))

    @Test fun everyTeslaStringMatchesTheSpecInBothLocales() {
        val english = localized("en")
        val chinese = localized("zh-CN")
        assertEquals(19, labels.size)
        labels.forEach { (id, texts) ->
            assertEquals(texts.first, english.getString(id))
            assertEquals(texts.second, chinese.getString(id))
        }
    }

    @Test fun addressHintFailureAndTeslaUrlFormatInBothLocales() {
        for (tag in listOf("en", "zh-CN")) {
            val context = localized(tag)
            for (address in listOf("169.254.88.1", "169.254.88.2", "3.3.3.2")) {
                val hint = context.getString(R.string.tesla_public_address_hint, address)
                assertTrue(hint.contains("https://$address:8443"))
                assertFalse(hint.contains("%1\$s"))
            }
            assertTrue(context.getString(R.string.tesla_address_failed, "raw cause").contains("raw cause"))
            assertTrue(context.getString(R.string.tesla_url, "https://3.3.3.2:9443/").endsWith("https://3.3.3.2:9443/"))
        }
    }
}
