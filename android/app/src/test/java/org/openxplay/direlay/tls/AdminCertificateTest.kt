// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-15
package org.openxplay.direlay.tls

import java.io.File
import java.security.KeyStore
import java.security.cert.X509Certificate
import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder

class AdminCertificateTest {
    @get:Rule val temporary = TemporaryFolder()
    @Test fun generatedCertificateIncludesDefaultAndConflictNamesAndMigratesOldMetadata() {
        val root = temporary.newFolder()
        val store = TlsStore(root)
        assertTrue(store.ensureDefault(setOf("192.168.1.2")))
        assertFalse(store.ensureDefault(setOf("192.168.1.2")))
        fun names(): Collection<List<*>> {
            val password = File(root, "tls-generated/password").readText().toCharArray()
            val keys = KeyStore.getInstance("PKCS12")
            File(root, "tls-generated/certificate.p12").inputStream().use { keys.load(it, password) }
            password.fill('\u0000')
            return (keys.getCertificate(keys.aliases().nextElement()) as X509Certificate).subjectAlternativeNames
        }
        assertTrue(names().any { it[0] == 2 && it[1] == "openxplay-android.local" })
        assertTrue(names().any { it[0] == 7 && it[1] == "192.168.1.2" })
        File(root, "tls-generated/addresses").writeText("192.168.1.2")
        assertTrue(store.ensureDefault(setOf("192.168.1.2")))
        assertTrue(store.ensureDefault(setOf("192.168.1.2"), setOf("openxplay-android-2.local")))
        assertTrue(names().any { it[0] == 2 && it[1] == "openxplay-android-2.local" })
    }
}
