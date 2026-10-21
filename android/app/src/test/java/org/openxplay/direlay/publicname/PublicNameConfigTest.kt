// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-20
package org.openxplay.direlay.publicname

import android.app.Application
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

/** org.json is Android's own implementation, so these run on Robolectric. */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [28], manifest = Config.NONE, application = Application::class)
class PublicNameConfigTest {
    private val duckToken = "a1b2c3d4-0000-0000-0000-000000000000"
    private fun parse(text: String, partial: Boolean = false) = PublicNameFile.parse(text.toByteArray(), partial)
    private fun valid(text: String) = (parse(text) as ConfigParse.Valid).config
    private fun errors(text: String) = (parse(text) as ConfigParse.Invalid).errors.map { it.key to it.problem.code }

    @Test fun validFilePerProvider() {
        val duck = valid("""{"version":1,"enabled":true,"provider":"duckdns","name":"myrelay.duckdns.org","token":"$duckToken","agreeLetsEncryptTerms":true}""")
        assertEquals(PublicNameConfig(true, DnsProviderId.DUCKDNS, "myrelay.duckdns.org", duckToken, true, AcmeEnvironment.PRODUCTION), duck)
        assertTrue(duck.complete)
        val dynv6 = valid("""{"version":1,"provider":"dynv6","name":"myrelay.dynv6.net","token":"Zx_9-token.value","agreeLetsEncryptTerms":true,"acme":"staging"}""")
        assertEquals(DnsProviderId.DYNV6, dynv6.provider)
        assertEquals(AcmeEnvironment.STAGING, dynv6.acme)
        // A file means "use it": enabled defaults to true; the terms are never agreed on the user's behalf.
        assertTrue(dynv6.enabled)
        val custom = valid("""{"version":1,"provider":"dynv6","name":"relay.example.com","token":"t"}""")
        assertFalse(custom.agreeTerms)
        assertFalse(custom.complete)
    }

    @Test fun namesAreLowerCasedAndCheckedPerProvider() {
        assertEquals("myrelay.duckdns.org", valid("""{"version":1,"provider":"duckdns","name":"MyRelay.DuckDNS.org","token":"$duckToken"}""").name)
        assertEquals(listOf("name" to "invalid"), errors("""{"version":1,"provider":"duckdns","name":"a.b.duckdns.org","token":"$duckToken"}"""))
        assertEquals(listOf("name" to "invalid"), errors("""{"version":1,"provider":"duckdns","name":"myrelay.example.org","token":"$duckToken"}"""))
        for (name in listOf("myrelay.dynv6.net.", "-bad.dynv6.net", "bad-.dynv6.net", "under_score.dynv6.net", "single", "",
            "x".repeat(64) + ".dynv6.net", (1..64).joinToString(".") { "abc" })) {
            assertEquals(name, listOf("name" to "invalid"), errors("""{"version":1,"provider":"dynv6","name":"$name","token":"t"}"""))
        }
        assertTrue(PublicNameRules.nameValid(DnsProviderId.DYNV6, (1..63).joinToString(".") { "abc" }.take(253).trimEnd('.')))
    }

    @Test fun tokensArePrintableAsciiAndDuckDnsTokensAreUuids() {
        assertEquals(listOf("token" to "invalid"), errors("""{"version":1,"provider":"duckdns","name":"a.duckdns.org","token":"not-a-uuid"}"""))
        assertEquals(listOf("token" to "invalid"), errors("""{"version":1,"provider":"dynv6","name":"a.dynv6.net","token":"has space"}"""))
        assertEquals(listOf("token" to "invalid"), errors("""{"version":1,"provider":"dynv6","name":"a.dynv6.net","token":"${"x".repeat(257)}"}"""))
        assertEquals(listOf("token" to "invalid"), errors("""{"version":1,"provider":"dynv6","name":"a.dynv6.net","token":"tök"}"""))
        assertEquals(listOf("token" to "invalid"), errors("""{"version":1,"provider":"dynv6","name":"a.dynv6.net","token":""}"""))
        assertEquals("x".repeat(256), valid("""{"version":1,"provider":"dynv6","name":"a.dynv6.net","token":"${"x".repeat(256)}"}""").token)
    }

    @Test fun everyErrorCodeNamesItsKey() {
        assertEquals(listOf("version" to "missing", "provider" to "missing", "name" to "missing", "token" to "missing"), errors("{}"))
        assertEquals(listOf("version" to "bad-version"), errors("""{"version":2,"provider":"dynv6","name":"a.dynv6.net","token":"t"}"""))
        assertEquals(listOf("version" to "bad-version"), errors("""{"version":"1","provider":"dynv6","name":"a.dynv6.net","token":"t"}"""))
        assertEquals(listOf("provider" to "invalid"), errors("""{"version":1,"provider":"noip","name":"a.dynv6.net","token":"t"}"""))
        assertEquals(listOf("enabled" to "invalid", "agreeLetsEncryptTerms" to "invalid", "acme" to "invalid"),
            errors("""{"version":1,"enabled":"yes","provider":"dynv6","name":"a.dynv6.net","token":"t","agreeLetsEncryptTerms":1,"acme":"test"}"""))
        assertEquals(listOf("name" to "invalid"), errors("""{"version":1,"provider":"dynv6","name":5,"token":"t"}"""))
        // Typos must not pass silently; unknown keys come after the known ones.
        assertEquals(listOf("agreeTerms" to "unknown-key", "tokn" to "unknown-key"),
            errors("""{"version":1,"provider":"dynv6","name":"a.dynv6.net","token":"t","tokn":"x","agreeTerms":true}"""))
    }

    @Test fun deSecIsNoLongerAProvider() {
        assertEquals(listOf("provider" to "invalid"), errors("""{"version":1,"provider":"desec","name":"myrelay.dedyn.io","token":"t"}"""))
        assertEquals(listOf("duckdns", "dynv6"), DnsProviderId.entries.map { it.key })
    }

    @Test fun fileLevelProblems() {
        assertEquals(listOf("file" to "not-json"), errors("not json"))
        assertEquals(listOf("file" to "not-json"), errors("[1,2]"))
        assertEquals(listOf("file" to "not-json"), errors("""{"version":1} trailing"""))
        assertEquals(listOf("file" to "not-json"),
            (PublicNameFile.parse(byteArrayOf('{'.code.toByte(), 0xC3.toByte(), '}'.code.toByte())) as ConfigParse.Invalid).errors.map { it.key to it.problem.code })
        val big = """{"version":1,"provider":"dynv6","name":"a.dynv6.net","token":"t","x":"${" ".repeat(PublicNameFile.MAX_BYTES)}"}"""
        assertEquals(listOf("file" to "too-large"), errors(big))
        val exact = """{"version":1,"provider":"dynv6","name":"a.dynv6.net","token":"t"}""".let { it.dropLast(1) + " ".repeat(PublicNameFile.MAX_BYTES - it.length) + "}" }
        assertEquals(PublicNameFile.MAX_BYTES, exact.length)
        assertTrue(parse(exact) is ConfigParse.Valid)
    }

    @Test fun storedFormMayBePartlyFilledAndRoundTrips() {
        val partial = PublicNameConfig(false, null, "", "", false)
        assertEquals(partial, (PublicNameFile.parse(PublicNameFile.encode(partial), partial = true) as ConfigParse.Valid).config)
        assertTrue(PublicNameFile.parse(PublicNameFile.encode(partial)) is ConfigParse.Invalid)
        val full = PublicNameConfig(true, DnsProviderId.DYNV6, "a.dynv6.net", "secret-token", true, AcmeEnvironment.STAGING)
        assertEquals(full, (PublicNameFile.parse(PublicNameFile.encode(full), partial = true) as ConfigParse.Valid).config)
    }

    @Test fun tokenNeverAppearsInDescriptions() {
        val config = PublicNameConfig(true, DnsProviderId.DYNV6, "a.dynv6.net", "secret-token", true)
        assertFalse(config.toString().contains("secret-token"))
        assertFalse(PublicNameEdit(true, DnsProviderId.DYNV6, "a.dynv6.net", "secret-token", true).toString().contains("secret-token"))
        assertFalse(HttpRequest("GET", "https://dynv6.com/api/update?token=secret-token").toString().contains("secret-token"))
        assertEquals("token-rejected", PublicNameFailure(PublicNameError.TOKEN_REJECTED).message)
    }
}
