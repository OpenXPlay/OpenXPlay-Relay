// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-20
package org.openxplay.direlay.publicname

import android.app.Application
import android.content.Context
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import org.openxplay.direlay.R
import org.openxplay.direlay.ui.publicNameErrorText
import org.openxplay.direlay.ui.publicNameImportProblems
import org.openxplay.direlay.ui.publicNameStatusText
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config
import java.util.Locale

/** The labels are shared with the Linux relay and the website, so they must match the design word for word. */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [33], qualifiers = "en", application = Application::class)
class PublicNameStringsTest {
    private fun localized(tag: String): Context {
        val context = RuntimeEnvironment.getApplication()
        val configuration = android.content.res.Configuration(context.resources.configuration).apply { setLocale(Locale.forLanguageTag(tag)) }
        return context.createConfigurationContext(configuration)
    }

    private val labels = mapOf(
        R.string.public_name_title to ("Trusted HTTPS" to "可信 HTTPS"),
        R.string.public_name_summary to ("Open the head-unit page without a certificate warning, using a free domain name." to "用免费域名，让车机打开网页时不再出现证书警告。"),
        R.string.public_name_enabled to ("Use a free domain name" to "使用免费域名"),
        R.string.public_name_provider to ("DNS provider" to "域名服务商"),
        R.string.public_name_name to ("Domain name" to "域名"),
        R.string.public_name_name_hint to ("For example myrelay.duckdns.org" to "例如 myrelay.duckdns.org"),
        R.string.public_name_token to ("Token" to "Token"),
        R.string.public_name_token_hint to ("Copy it from your provider account. It stays on this relay." to "从服务商账号中复制，只保存在这台中继上。"),
        R.string.public_name_token_saved to ("Token saved" to "已保存 Token"),
        R.string.public_name_terms to ("I agree to the Let's Encrypt Subscriber Agreement" to "我同意 Let's Encrypt 订户协议"),
        R.string.public_name_import to ("Import configuration file" to "导入配置文件"),
        R.string.public_name_save to ("Save" to "保存"),
        R.string.public_name_renew to ("Request certificate now" to "立即申请证书"),
        R.string.public_name_remove to ("Remove" to "移除"),
        R.string.public_name_status_off to ("Off" to "未开启"),
        R.string.public_name_status_incomplete to ("Fill in the provider, domain name and token, and agree to the terms." to "请填写域名服务商、域名和 Token，并同意协议。"),
        R.string.public_name_status_waiting_network to ("Waiting for internet access" to "等待联网"),
        R.string.public_name_status_updating_address to ("Updating the domain's address…" to "正在更新域名地址…"),
        R.string.public_name_status_requesting_certificate to ("Requesting a certificate…" to "正在申请证书…"),
        R.string.public_name_error_token_rejected to ("The provider rejected the token" to "服务商拒绝了这个 Token"),
        R.string.public_name_error_name_not_found to ("The provider doesn't know this domain name" to "服务商找不到这个域名"),
        R.string.public_name_error_provider_rate_limited to ("The provider asked to wait" to "服务商要求稍后再试"),
        R.string.public_name_error_provider_unreachable to ("Can't reach the provider" to "无法连接服务商"),
        R.string.public_name_error_dns_not_updated to ("The provider's name servers didn't show the change in time" to "服务商的域名服务器未及时生效"),
        R.string.public_name_error_acme_rate_limited to ("Let's Encrypt asked to wait" to "Let's Encrypt 要求稍后再试"),
        R.string.public_name_error_no_address to ("No head-unit network address yet" to "还没有车机网络地址"),
    )

    @Test fun labelsMatchTheSharedTableInBothLanguages() {
        val english = localized("en")
        val chinese = localized("zh-CN")
        labels.forEach { (id, texts) ->
            assertEquals(texts.first, english.getString(id))
            assertEquals(texts.second, chinese.getString(id))
        }
        assertEquals("Ready: https://myrelay.duckdns.org:8443/ (certificate valid until 1 Dec 2026)",
            english.getString(R.string.public_name_status_ready, "https://myrelay.duckdns.org:8443/", "1 Dec 2026"))
        assertEquals("已就绪：https://myrelay.duckdns.org:8443/（证书有效期至 2026年12月1日）",
            chinese.getString(R.string.public_name_status_ready, "https://myrelay.duckdns.org:8443/", "2026年12月1日"))
        assertEquals("Problem: The provider rejected the token. Next try at 14:05.",
            english.getString(R.string.public_name_status_error, english.getString(R.string.public_name_error_token_rejected), "14:05"))
        assertEquals("出现问题：服务商拒绝了这个 Token。将于 14:05 重试。",
            chinese.getString(R.string.public_name_status_error, chinese.getString(R.string.public_name_error_token_rejected), "14:05"))
        assertEquals("Let's Encrypt rejected the request: badCSR", english.getString(R.string.public_name_error_acme_rejected, "badCSR"))
        assertEquals("Let's Encrypt 拒绝了请求：badCSR", chinese.getString(R.string.public_name_error_acme_rejected, "badCSR"))
    }

    @Test fun statusLineShowsTheFullAddressAndTheCasDetail() {
        val english = localized("en")
        val ready = publicNameStatusText(english, PublicNameStatus(PublicNameState.READY, "myrelay.duckdns.org", "192.168.43.1",
            notAfter = 1_800_000_000_000L, trustedName = "myrelay.duckdns.org"), "https://myrelay.duckdns.org:8443/")
        assertTrue(ready, ready.startsWith("Ready: https://myrelay.duckdns.org:8443/ (certificate valid until "))
        val error = publicNameStatusText(english, PublicNameStatus(PublicNameState.ERROR, "myrelay.duckdns.org",
            error = PublicNameError.ACME_REJECTED, detail = "Incorrect TXT record", nextAttempt = System.currentTimeMillis() + 60_000), null)
        assertTrue(error, error.startsWith("Problem: Let's Encrypt rejected the request: Incorrect TXT record. Next try at "))
        assertEquals("Can't reach the provider", publicNameErrorText(english, null, null))
        assertEquals("Off", publicNameStatusText(english, PublicNameStatus(PublicNameState.OFF), null))
    }

    @Test fun importProblemsNameTheKeyAndTheStableCode() {
        val english = localized("en")
        assertEquals("token: invalid value (invalid)\ntokn: unknown key (unknown-key)\nFile: larger than 16 KiB (too-large)",
            publicNameImportProblems(english, listOf(ConfigError("token", ConfigProblem.INVALID), ConfigError("tokn", ConfigProblem.UNKNOWN_KEY),
                ConfigError(ConfigError.FILE, ConfigProblem.TOO_LARGE))))
        assertEquals("version：版本必须为 1（bad-version）",
            publicNameImportProblems(localized("zh-CN"), listOf(ConfigError("version", ConfigProblem.BAD_VERSION))))
    }
}
