// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-20
package org.openxplay.direlay.publicname

import org.json.JSONArray
import org.json.JSONObject
import org.openxplay.direlay.storage.PrivateStore
import java.io.File
import java.security.KeyFactory
import java.security.KeyPair
import java.security.KeyPairGenerator
import java.security.spec.ECGenParameterSpec
import java.security.spec.PKCS8EncodedKeySpec
import java.security.spec.X509EncodedKeySpec

/** Renewal information for one certificate; [renewAt] is null when the CA offers no ARI. */
data class AriState(val certId: String, val renewAt: Long?, val windowStart: Long?, val windowEnd: Long?, val nextPoll: Long)

/** Scheduler facts that must survive a restart: recent new orders (for the local guard) and ARI. */
data class PublicNameRecord(val orders: List<Pair<String, Long>> = emptyList(), val ari: AriState? = null) {
    fun orders(name: String): List<Long> = orders.filter { it.first == name }.map { it.second }
}

/** Only these fields belong to a provider; tokens stay in the private store. */
data class PublicNameProviderSetup(val name: String, val token: String) {
    override fun toString() = "PublicNameProviderSetup(name=$name, token=${if (token.isEmpty()) "none" else "set"})"
}

/**
 * Configuration, token and ACME account key in `no_backup/public-name/` (0700, files 0600).
 * Never part of RelaySettings, a saved-state Bundle or the admin API.
 */
class PublicNameStore(noBackupFilesDir: File) {
    private val store = PrivateStore(noBackupFilesDir, "public-name")

    @Synchronized fun config(): PublicNameConfig? {
        val config = readConfig()
        readProviders(config)
        return config
    }

    /** Saved provider credentials, for the service only; the screen gets names and token-present flags. */
    @Synchronized fun savedProviders(): Map<DnsProviderId, PublicNameProviderSetup> = readProviders(readConfig())

    private fun readConfig(): PublicNameConfig? {
        val bytes = read(CONFIG, PublicNameFile.MAX_BYTES) ?: return null
        return try { (PublicNameFile.parse(bytes, partial = true) as? ConfigParse.Valid)?.config } finally { bytes.fill(0) }
    }

    /** Activates this configuration and replaces only its provider's setup, in one private generation. */
    @Synchronized fun save(config: PublicNameConfig) {
        val providers = readProviders(readConfig()).toMutableMap()
        config.provider?.let { providers[it] = PublicNameProviderSetup(config.name, config.token) }
        update(mapOf(CONFIG to PublicNameFile.encode(config), PROVIDERS to encodeProviders(providers)))
    }

    /** Removes every provider's setup, the active configuration, the account key and the record. */
    @Synchronized fun remove() = store.remove()

    /** Older installs have only config.json; seed its provider without changing that version-1 file. */
    private fun readProviders(config: PublicNameConfig?): Map<DnsProviderId, PublicNameProviderSetup> {
        val bytes = read(PROVIDERS, PublicNameFile.MAX_BYTES)
        if (bytes != null) try {
            val parsed = runCatching {
                val json = JSONObject(String(bytes, Charsets.UTF_8))
                require(json.getInt("version") == 1)
                val providers = json.getJSONObject("providers")
                DnsProviderId.entries.mapNotNull { provider ->
                    if (!providers.has(provider.key)) return@mapNotNull null
                    val entry = providers.getJSONObject(provider.key)
                    val name = entry.getString("name")
                    val token = entry.getString("token")
                    require(name.isEmpty() || PublicNameRules.nameValid(provider, name))
                    require(token.isEmpty() || PublicNameRules.tokenValid(provider, token))
                    provider to PublicNameProviderSetup(name, token)
                }.toMap()
            }.getOrNull()
            if (parsed != null) return parsed
        } finally { bytes.fill(0) }
        val migrated = config?.provider?.let { mapOf(it to PublicNameProviderSetup(config.name, config.token)) }.orEmpty()
        if (config != null) update(mapOf(PROVIDERS to encodeProviders(migrated)))
        return migrated
    }

    private fun encodeProviders(providers: Map<DnsProviderId, PublicNameProviderSetup>): ByteArray {
        val entries = JSONObject()
        providers.forEach { (provider, setup) ->
            entries.put(provider.key, JSONObject().put("name", setup.name).put("token", setup.token))
        }
        return JSONObject().put("version", 1).put("providers", entries).toString().toByteArray(Charsets.UTF_8)
    }

    /** The account key is created once and kept with the configuration; null without a configuration. */
    @Synchronized fun accountKey(): KeyPair? {
        if (!configured()) return null
        val privateBytes = read(ACCOUNT_KEY, 4096)
        val publicBytes = read(ACCOUNT_PUBLIC, 4096)
        if (privateBytes != null && publicBytes != null) try {
            val factory = KeyFactory.getInstance("EC")
            return KeyPair(factory.generatePublic(X509EncodedKeySpec(publicBytes)), factory.generatePrivate(PKCS8EncodedKeySpec(privateBytes)))
        } catch (_: Exception) { /* Replaced below; the account is then registered again. */ }
        finally { privateBytes.fill(0) }
        val pair = KeyPairGenerator.getInstance("EC").apply { initialize(ECGenParameterSpec("secp256r1")) }.generateKeyPair()
        update(mapOf(ACCOUNT_KEY to pair.private.encoded, ACCOUNT_PUBLIC to pair.public.encoded, ACCOUNT to null))
        return pair
    }

    /** The account URL (`kid`) registered with this ACME directory, if any. */
    @Synchronized fun accountUrl(directory: String): String? {
        val bytes = read(ACCOUNT, 4096) ?: return null
        return runCatching { JSONObject(String(bytes, Charsets.UTF_8)) }.getOrNull()
            ?.takeIf { it.optString("directory") == directory }?.optString("url")?.takeIf { it.isNotEmpty() }
    }

    @Synchronized fun saveAccountUrl(directory: String, url: String?) {
        if (!configured()) return
        update(mapOf(ACCOUNT to url?.let { JSONObject().put("directory", directory).put("url", it).toString().toByteArray() }))
    }

    @Synchronized fun record(): PublicNameRecord {
        val bytes = read(RECORD, 64 * 1024) ?: return PublicNameRecord()
        return runCatching {
            val json = JSONObject(String(bytes, Charsets.UTF_8))
            val orders = json.optJSONArray("orders")?.let { array ->
                (0 until array.length()).map { array.getJSONObject(it).let { o -> o.getString("name") to o.getLong("at") } }
            }.orEmpty()
            val ari = json.optJSONObject("ari")?.let {
                AriState(it.getString("certId"), it.optLongOrNull("renewAt"), it.optLongOrNull("start"),
                    it.optLongOrNull("end"), it.getLong("nextPoll"))
            }
            PublicNameRecord(orders, ari)
        }.getOrDefault(PublicNameRecord())
    }

    @Synchronized fun saveRecord(record: PublicNameRecord) {
        if (!configured()) return
        val orders = JSONArray()
        record.orders.takeLast(MAX_ORDERS).forEach { (name, at) -> orders.put(JSONObject().put("name", name).put("at", at)) }
        val json = JSONObject().put("orders", orders)
        record.ari?.let {
            json.put("ari", JSONObject().put("certId", it.certId).put("renewAt", it.renewAt ?: JSONObject.NULL)
                .put("start", it.windowStart ?: JSONObject.NULL).put("end", it.windowEnd ?: JSONObject.NULL).put("nextPoll", it.nextPoll))
        }
        update(mapOf(RECORD to json.toString().toByteArray()))
    }

    private fun configured() = File(store.directory, CONFIG).isFile

    private fun read(name: String, limit: Int): ByteArray? {
        val file = File(store.directory, name)
        if (!file.isFile || file.length() > limit) return null
        return file.readBytes()
    }

    /** Every change publishes a complete new generation; null removes a file. */
    private fun update(changes: Map<String, ByteArray?>) {
        val current = store.directory.listFiles().orEmpty().filter { it.isFile && it.name !in changes }
            .associate { it.name to it.readBytes() }
        try {
            store.replace { staging ->
                (current + changes).forEach { (name, bytes) -> if (bytes != null) PrivateStore.write(staging, name, bytes) }
            }
        } finally {
            current.values.forEach { it.fill(0) }
            changes.values.forEach { it?.fill(0) }
        }
    }

    private fun JSONObject.optLongOrNull(key: String): Long? = if (isNull(key)) null else optLong(key)

    companion object {
        private const val CONFIG = "config.json"
        private const val PROVIDERS = "providers.json"
        private const val ACCOUNT_KEY = "account.key"
        private const val ACCOUNT_PUBLIC = "account.pub"
        private const val ACCOUNT = "account.json"
        private const val RECORD = "record.json"
        private const val MAX_ORDERS = 32
    }
}
