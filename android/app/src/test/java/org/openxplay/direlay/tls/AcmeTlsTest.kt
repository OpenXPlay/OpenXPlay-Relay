// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-20
package org.openxplay.direlay.tls

import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder
import org.openxplay.direlay.SyntheticIdentity
import org.openxplay.direlay.publicname.TestCa
import java.io.File
import java.net.InetAddress
import java.net.ServerSocket
import java.nio.ByteBuffer
import java.nio.file.Files
import java.nio.file.attribute.PosixFilePermissions
import java.security.cert.X509Certificate
import javax.net.ssl.SNIHostName
import javax.net.ssl.SSLContext
import javax.net.ssl.SSLEngine
import javax.net.ssl.SSLEngineResult.HandshakeStatus
import javax.net.ssl.SSLEngineResult.Status
import javax.net.ssl.SSLSocket
import javax.net.ssl.TrustManager
import javax.net.ssl.X509TrustManager
import kotlin.concurrent.thread

/** SNI selection with the JVM's TLS stack (the Redmi check covers Conscrypt) and the ACME source in the store. */
class AcmeTlsTest {
    @get:Rule val temporary = TemporaryFolder()
    private val name = "myrelay.duckdns.org"
    private val loopback: InetAddress = InetAddress.getByName("127.0.0.1")
    private var now = System.currentTimeMillis()
    private var changes = 0
    private val ca = TestCa("Fake Let's Encrypt")
    private val acmeKey = TestCa.keyPair()
    private val acmeLeaf = ca.leaf(name, acmeKey.public)

    private fun store(root: File = temporary.newFolder()) = TlsStore(root, { now }) { changes++ }

    private val trustAll = SSLContext.getInstance("TLS").apply {
        init(null, arrayOf<TrustManager>(object : X509TrustManager {
            override fun checkClientTrusted(chain: Array<out X509Certificate>?, authType: String?) = Unit
            override fun checkServerTrusted(chain: Array<out X509Certificate>?, authType: String?) = Unit
            override fun getAcceptedIssuers(): Array<X509Certificate> = emptyArray()
        }), null)
    }

    private fun SSLSocket.requestName(sni: String?) { if (sni != null) sslParameters = sslParameters.also { it.serverNames = listOf(SNIHostName(sni)) } }
    private fun SSLEngine.requestName(sni: String?) { if (sni != null) sslParameters = sslParameters.also { it.serverNames = listOf(SNIHostName(sni)) } }

    /** The certificate chain a client sees over an SSLEngine pair, or over the admin (server socket) or web (layered) path. */
    private fun served(context: SSLContext, sni: String?, path: String): List<X509Certificate> = when (path) {
        "engine" -> engine(context, sni)
        else -> socket(context, sni, layered = path == "layered")
    }

    private fun socket(context: SSLContext, sni: String?, layered: Boolean): List<X509Certificate> {
        val listener = if (layered) ServerSocket(0, 1, loopback) else context.serverSocketFactory.createServerSocket(0, 1, loopback)
        listener.use {
            val server = thread {
                runCatching {
                    val accepted = listener.accept()
                    val secured = if (layered) (context.socketFactory.createSocket(accepted, null, accepted.port, true) as SSLSocket)
                        .apply { useClientMode = false } else accepted as SSLSocket
                    secured.use { it.startHandshake(); it.outputStream.write(1); it.outputStream.flush(); it.inputStream.read() }
                }
            }
            (trustAll.socketFactory.createSocket(loopback, listener.localPort) as SSLSocket).use { client ->
                client.requestName(sni)
                client.startHandshake()
                client.inputStream.read()
                return client.session.peerCertificates.map { it as X509Certificate }.also { server.join(5_000) }
            }
        }
    }

    private fun engine(context: SSLContext, sni: String?): List<X509Certificate> {
        val client = trustAll.createSSLEngine().apply { useClientMode = true; requestName(sni) }
        val server = context.createSSLEngine().apply { useClientMode = false }
        val toServer = ByteBuffer.allocate(1 shl 17)
        val toClient = ByteBuffer.allocate(1 shl 17)
        val app = ByteBuffer.allocate(1 shl 17)
        val empty = ByteBuffer.allocate(0)
        fun drive(engine: SSLEngine, inbound: ByteBuffer, outbound: ByteBuffer) {
            while (true) when (engine.handshakeStatus) {
                HandshakeStatus.NEED_TASK -> while (true) (engine.delegatedTask ?: break).run()
                HandshakeStatus.NEED_WRAP -> engine.wrap(empty, outbound)
                HandshakeStatus.NEED_UNWRAP -> {
                    inbound.flip()
                    val result = engine.unwrap(inbound, app)
                    inbound.compact()
                    app.clear()
                    if (result.status == Status.BUFFER_UNDERFLOW || result.bytesConsumed() == 0 && result.handshakeStatus == HandshakeStatus.NEED_UNWRAP) return
                }
                else -> return
            }
        }
        client.beginHandshake()
        server.beginHandshake()
        repeat(64) {
            drive(client, toClient, toServer)
            drive(server, toServer, toClient)
            if (client.handshakeStatus == HandshakeStatus.NOT_HANDSHAKING && server.handshakeStatus == HandshakeStatus.NOT_HANDSHAKING)
                return client.session.peerCertificates.map { it as X509Certificate }
        }
        throw AssertionError("handshake did not finish")
    }

    @Test fun theNameGetsTheLetsEncryptChainAndEveryOtherClientTodaysCertificate() {
        val tls = store()
        tls.ensureDefault(setOf("192.168.43.1"))
        val generated = tls.fingerprint()
        tls.installAcme(name, listOf(acmeLeaf, ca.certificate), acmeKey.private)
        tls.serveAcme(name)
        val context = tls.webContext(setOf("192.168.43.1"))
        for (path in listOf("engine", "admin", "layered")) {
            assertEquals(path, listOf(acmeLeaf, ca.certificate), served(context, name, path))
            assertEquals(path, acmeLeaf, served(context, "MyRelay.DuckDNS.org", path).first())
            for (other in listOf("other.duckdns.org", null)) {
                val fallback = served(context, other, path)
                assertEquals("$path $other", generated, fingerprint(fallback.first()))
            }
        }
    }

    @Test fun offOrMissingServesExactlyTodaysCertificate() {
        val tls = store()
        tls.ensureDefault(setOf("192.168.43.1"))
        tls.serveAcme(name)
        assertEquals("nothing to serve without a certificate", 0, changes)
        tls.installAcme(name, listOf(acmeLeaf, ca.certificate), acmeKey.private)
        tls.serveAcme(null)
        assertEquals(tls.fingerprint(), fingerprint(served(tls.sslContext()!!, name, "engine").first()))
        assertEquals(tls.fingerprint(), tls.revision())
        tls.serveAcme("another.duckdns.org")
        assertEquals("a certificate for another name is never served", tls.fingerprint(), fingerprint(served(tls.sslContext()!!, name, "engine").first()))
    }

    @Test fun anExpiredCertificateFallsBackEvenInAnExistingContext() {
        val tls = store()
        tls.ensureDefault(setOf("192.168.43.1"))
        tls.installAcme(name, listOf(acmeLeaf, ca.certificate), acmeKey.private)
        tls.serveAcme(name)
        val context = checkNotNull(tls.sslContext())
        val served = tls.revision()
        now = acmeLeaf.notAfter.time + 1
        assertEquals(tls.fingerprint(), fingerprint(served(context, name, "engine").first()))
        assertNotEquals("servers restart on the revision", served, tls.revision())
        assertEquals(tls.fingerprint(), tls.revision())
    }

    @Test fun developerRemoveDeletesOnlyImportedMaterialAndImportedStillWinsWithoutTheName() {
        val root = temporary.newFolder()
        val tls = store(root)
        tls.ensureDefault(setOf("192.168.43.1"))
        tls.importFrom(SyntheticIdentity().pkcs12(charArrayOf()), charArrayOf())
        tls.installAcme(name, listOf(acmeLeaf, ca.certificate), acmeKey.private)
        tls.serveAcme(name)
        val imported = tls.fingerprint()
        assertEquals(imported, fingerprint(served(tls.sslContext()!!, null, "engine").first()))
        assertEquals(acmeLeaf, served(tls.sslContext()!!, name, "engine").first())
        tls.remove()
        assertFalse(tls.hasImported())
        assertEquals(name, tls.acmeMaterial()?.name)
        assertEquals(acmeLeaf, served(tls.sslContext()!!, name, "engine").first())
        // The ACME files are private and separate from the imported and generated ones.
        val directory = File(root, "tls-acme").canonicalFile
        assertEquals("rwx------", PosixFilePermissions.toString(Files.getPosixFilePermissions(directory.toPath())))
        assertEquals(setOf("name", "chain.pem", "key.pk8"), directory.listFiles().orEmpty().map { it.name }.toSet())
        directory.listFiles().orEmpty().forEach { assertEquals("rw-------", PosixFilePermissions.toString(Files.getPosixFilePermissions(it.toPath()))) }
        tls.removeAcme()
        assertNull(tls.acmeMaterial())
        assertFalse(File(root, "tls-acme").exists())
        assertTrue(root.listFiles().orEmpty().none { it.name.contains("tls-acme") })
    }

    @Test fun installValidatesTheChainAndEveryServedChangeIsAnnounced() {
        val tls = store()
        tls.ensureDefault(setOf("192.168.43.1"))
        tls.installAcme(name, listOf(acmeLeaf, ca.certificate), acmeKey.private)
        assertEquals(1, changes)
        tls.serveAcme(name)
        assertEquals(2, changes)
        tls.serveAcme(name)
        assertEquals("unchanged", 2, changes)
        // A key that does not belong to the leaf, or a leaf for another name, never replaces the stored one.
        assertThrows(IllegalArgumentException::class.java) { tls.installAcme(name, listOf(acmeLeaf), TestCa.keyPair().private) }
        assertThrows(IllegalArgumentException::class.java) { tls.installAcme("other.duckdns.org", listOf(acmeLeaf), acmeKey.private) }
        assertEquals(acmeLeaf, tls.acmeMaterial()?.leaf)
        val renewedKey = TestCa.keyPair()
        val renewed = ca.leaf(name, renewedKey.public)
        val before = tls.revision()
        tls.installAcme(name, listOf(renewed, ca.certificate), renewedKey.private)
        assertEquals(3, changes)
        assertNotEquals(before, tls.revision())
        assertEquals(renewed, served(tls.sslContext()!!, name, "engine").first())
        tls.serveAcme(null)
        assertEquals(4, changes)
        tls.removeAcme()
        assertEquals(5, changes)
        tls.removeAcme()
        assertEquals("nothing left to remove", 5, changes)
        // A damaged file reads as no certificate.
        val root = temporary.newFolder()
        val damaged = store(root)
        damaged.installAcme(name, listOf(acmeLeaf), acmeKey.private)
        File(File(root, "tls-acme").canonicalFile, "chain.pem").writeText("garbage")
        assertNull(store(root).acmeMaterial())
    }

    private fun fingerprint(certificate: X509Certificate) = java.security.MessageDigest.getInstance("SHA-256")
        .digest(certificate.encoded).joinToString(":") { "%02X".format(it.toInt() and 255) }
}
