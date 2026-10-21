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
    }

    @Test fun removeDeletesConfigurationTokenAccountKeyAndRecord() {
        val root = temporary.newFolder()
        val store = PublicNameStore(root)
        store.save(config)
        store.accountKey()
        store.saveRecord(PublicNameRecord(listOf("myrelay.dynv6.net" to 5L)))
        store.remove()
        assertNull(store.config())
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
