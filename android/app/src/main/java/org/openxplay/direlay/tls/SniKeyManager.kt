// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-20
package org.openxplay.direlay.tls

import java.net.Socket
import java.security.Principal
import java.security.PrivateKey
import java.security.cert.X509Certificate
import java.util.Locale
import javax.net.ssl.ExtendedSSLSession
import javax.net.ssl.SNIHostName
import javax.net.ssl.SSLEngine
import javax.net.ssl.SSLSession
import javax.net.ssl.SSLSocket
import javax.net.ssl.StandardConstants
import javax.net.ssl.X509ExtendedKeyManager
import javax.net.ssl.X509KeyManager

/**
 * Serves the public name's certificate when the client asks for that name (SNI) and it is still valid;
 * every other connection, IP-address access included, gets [fallback] exactly as before.
 */
class SniKeyManager(name: String, private val key: PrivateKey, chain: List<X509Certificate>,
    private val fallback: X509KeyManager, private val now: () -> Long = System::currentTimeMillis) : X509ExtendedKeyManager() {
    private val name = name.lowercase(Locale.ROOT)
    private val chain = chain.toTypedArray()
    private val notAfter = chain.first().notAfter.time

    override fun chooseServerAlias(keyType: String?, issuers: Array<out Principal>?, socket: Socket?): String? =
        choose(keyType, (socket as? SSLSocket)?.let { runCatching { it.handshakeSession }.getOrNull() }) {
            fallback.chooseServerAlias(keyType, issuers, socket)
        }

    override fun chooseEngineServerAlias(keyType: String?, issuers: Array<out Principal>?, engine: SSLEngine?): String? =
        choose(keyType, engine?.let { runCatching { it.handshakeSession }.getOrNull() }) {
            (fallback as? X509ExtendedKeyManager)?.chooseEngineServerAlias(keyType, issuers, engine)
                ?: fallback.chooseServerAlias(keyType, issuers, null)
        }

    /** With the name requested, only the ACME certificate may be chosen: a second key type must not replace it. */
    private fun choose(keyType: String?, session: SSLSession?, default: () -> String?): String? {
        if (requestedName(session) != name || now() >= notAfter) return default()
        return if (key.algorithm == "EC" && keyType in EC_KEY_TYPES) ALIAS else null
    }

    override fun getCertificateChain(alias: String?): Array<X509Certificate>? =
        if (alias == ALIAS) chain.clone() else fallback.getCertificateChain(alias)
    override fun getPrivateKey(alias: String?): PrivateKey? = if (alias == ALIAS) key else fallback.getPrivateKey(alias)
    override fun getServerAliases(keyType: String?, issuers: Array<out Principal>?): Array<String>? {
        val aliases = fallback.getServerAliases(keyType, issuers).orEmpty().toList() +
            listOfNotNull(ALIAS.takeIf { key.algorithm == "EC" && keyType in EC_KEY_TYPES })
        return aliases.takeIf { it.isNotEmpty() }?.toTypedArray()
    }
    override fun getClientAliases(keyType: String?, issuers: Array<out Principal>?): Array<String>? = fallback.getClientAliases(keyType, issuers)
    override fun chooseClientAlias(keyType: Array<out String>?, issuers: Array<out Principal>?, socket: Socket?): String? =
        fallback.chooseClientAlias(keyType, issuers, socket)

    companion object {
        const val ALIAS = "openxplay-public-name"
        private val EC_KEY_TYPES = setOf("EC", "EC_EC", "ECDSA")

        fun requestedName(session: SSLSession?): String? = runCatching {
            (session as? ExtendedSSLSession)?.requestedServerNames?.firstOrNull { it.type == StandardConstants.SNI_HOST_NAME }
                ?.let { (it as? SNIHostName)?.asciiName ?: String(it.encoded, Charsets.US_ASCII) }?.lowercase(Locale.ROOT)
        }.getOrNull()
    }
}
