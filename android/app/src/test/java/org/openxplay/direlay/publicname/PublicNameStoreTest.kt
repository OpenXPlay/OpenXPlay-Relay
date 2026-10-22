// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-20
package org.openxplay.direlay.publicname

import android.app.Application
import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder
import org.junit.runner.RunWith
import org.json.JSONObject
import org.openxplay.direlay.storage.PrivateStore
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import java.io.File
import java.nio.file.Files
import java.nio.file.attribute.PosixFilePermissions

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [28], manifest = Config.NONE, application = Application::class)
class PublicNameStoreTest {
    @get:Rule val temporary = TemporaryFolder()
    private val config = PublicNameConfig(true, DnsProviderId.DYNV6, "myrelay.dynv6.net", "secret-token", true)

    @Test fun configurationTokenAndAccountKeyStayPrivateAndSurviveReload() {
        val root = temporary.newFolder()
        val store = PublicNameStore(root)
        assertNull(store.config())
        assertTrue(store.savedProviders().isEmpty())
        assertNull("no account key without a configuration", store.accountKey())
        store.save(config)
        val key = checkNotNull(store.accountKey())
        store.saveAccountUrl(AcmeEnvironment.PRODUCTION.directory, "https://acme.test/acct/1")
        store.saveRecord(PublicNameRecord(listOf("myrelay.dynv6.net" to 5L), AriState("a.b", 10, 1, 20, 30)))
        val reloaded = PublicNameStore(root)
        assertEquals(config, reloaded.config())
        assertEquals(key.public, reloaded.accountKey()?.public)
        assertEquals("https://acme.test/acct/1", reloaded.accountUrl(AcmeEnvironment.PRODUCTION.directory))
        assertNull("another directory registers its own account", reloaded.accountUrl(AcmeEnvironment.STAGING.directory))
        assertEquals(listOf(5L), reloaded.record().orders("myrelay.dynv6.net"))
        assertEquals(AriState("a.b", 10, 1, 20, 30), reloaded.record().ari)
        // Saving the settings again keeps the account key and the record.
        reloaded.save(config.copy(enabled = false))
        assertEquals(key.public, PublicNameStore(root).accountKey()?.public)
        assertEquals(listOf(5L), PublicNameStore(root).record().orders("myrelay.dynv6.net"))

        val directory = File(root, "public-name").canonicalFile
        assertEquals("rwx------", PosixFilePermissions.toString(Files.getPosixFilePermissions(directory.toPath())))
        directory.listFiles().orEmpty().forEach {
            assertEquals(it.name, "rw-------", PosixFilePermissions.toString(Files.getPosixFilePermissions(it.toPath())))
        }
        assertTrue(File(directory, "config.json").readText().contains("secret-token"))
        assertTrue(File(directory, "providers.json").readText().contains("secret-token"))
    }

    @Test fun firstReadMigratesTheSingleConfigWithoutChangingItsFormatOrSharedFiles() {
        val root = temporary.newFolder()
        val legacy = PrivateStore(root, "public-name")
        val bytes = PublicNameFile.encode(config.copy(acme = AcmeEnvironment.STAGING))
        legacy.replace { directory ->
            PrivateStore.write(directory, "config.json", bytes)
            PrivateStore.write(directory, "account.json", """{"directory":"test","url":"https://acme.test/acct/1"}""".toByteArray())
            PrivateStore.write(directory, "record.json", """{"orders":[{"name":"myrelay.dynv6.net","at":5}]}""".toByteArray())
        }
        val store = PublicNameStore(root)
        assertEquals(config.copy(acme = AcmeEnvironment.STAGING), store.config())
        assertTrue("config() performs the migration", File(legacy.directory, "providers.json").isFile)
        assertArrayEquals(bytes, File(legacy.directory, "config.json").readBytes())
        assertEquals(mapOf(DnsProviderId.DYNV6 to PublicNameProviderSetup(config.name, config.token)), store.savedProviders())
        assertEquals("https://acme.test/acct/1", store.accountUrl("test"))
        assertEquals(listOf(5L), store.record().orders(config.name))
        val generation = legacy.directory.canonicalPath
        assertEquals(store.savedProviders(), PublicNameStore(root).savedProviders())
        assertEquals("migration is not repeated", generation, legacy.directory.canonicalPath)
    }

    @Test fun readingProvidersFirstAlsoMigratesAnIncompleteLegacySetup() {
        val root = temporary.newFolder()
        PrivateStore(root, "public-name").replace {
            PrivateStore.write(it, "config.json", PublicNameFile.encode(config.copy(token = "", agreeTerms = false)))
        }
        assertEquals(mapOf(DnsProviderId.DYNV6 to PublicNameProviderSetup(config.name, "")), PublicNameStore(root).savedProviders())
        assertEquals(config.copy(token = "", agreeTerms = false), PublicNameStore(root).config())
    }

    @Test fun activatingOneProviderKeepsTheOtherSetupAndTheSharedAccountAndRecord() {
        val root = temporary.newFolder()
        val store = PublicNameStore(root)
        store.save(config)
        val key = checkNotNull(store.accountKey())
        store.saveRecord(PublicNameRecord(listOf(config.name to 5L)))
        val duck = PublicNameConfig(false, DnsProviderId.DUCKDNS, "other.duckdns.org",
            "a1b2c3d4-0000-0000-0000-000000000000", false, AcmeEnvironment.STAGING)
        store.save(duck)
        val setups = mapOf(DnsProviderId.DUCKDNS to PublicNameProviderSetup(duck.name, duck.token),
            DnsProviderId.DYNV6 to PublicNameProviderSetup(config.name, config.token))
        val reloaded = PublicNameStore(root)
        assertEquals(duck, reloaded.config())
        assertEquals(setups, reloaded.savedProviders())
        assertEquals(key.public, reloaded.accountKey()?.public)
        assertEquals(listOf(5L), reloaded.record().orders(config.name))
        val active = JSONObject(File(root, "public-name/config.json").readText())
        assertEquals(1, active.getInt("version"))
        val providers = JSONObject(File(root, "public-name/providers.json").readText()).getJSONObject("providers")
        DnsProviderId.entries.forEach {
            assertEquals(setOf("name", "token"), providers.getJSONObject(it.key).keys().asSequence().toSet())
        }
        reloaded.save(config)
        assertEquals(config, reloaded.config())
        assertEquals(setups, reloaded.savedProviders())
        assertFalse(reloaded.savedProviders().toString().contains(config.token))
        assertFalse(reloaded.savedProviders().toString().contains(duck.token))
    }

    @Test fun removeDeletesEveryProviderConfigurationTokenAccountKeyAndRecord() {
        val root = temporary.newFolder()
        val store = PublicNameStore(root)
        store.save(config)
        store.save(PublicNameConfig(false, DnsProviderId.DUCKDNS, "other.duckdns.org",
            "a1b2c3d4-0000-0000-0000-000000000000", true))
        assertEquals(2, store.savedProviders().size)
        store.accountKey()
        store.saveRecord(PublicNameRecord(listOf("myrelay.dynv6.net" to 5L)))
        store.remove()
        assertNull(store.config())
        assertTrue(store.savedProviders().isEmpty())
        assertFalse(File(root, "public-name").exists())
        assertTrue("no generation is left behind", root.listFiles().orEmpty().none { it.name.contains("public-name") })
        store.saveRecord(PublicNameRecord(listOf("myrelay.dynv6.net" to 6L)))
        store.saveAccountUrl(AcmeEnvironment.PRODUCTION.directory, "https://acme.test/acct/2")
        assertFalse("nothing is written back after removal", File(root, "public-name").exists())
    }

    @Test fun damagedFilesReadAsAbsent() {
        val root = temporary.newFolder()
        val store = PublicNameStore(root)
        store.save(config)
        File(File(root, "public-name").canonicalFile, "config.json").writeText("{")
        assertNull(store.config())
    }
}
