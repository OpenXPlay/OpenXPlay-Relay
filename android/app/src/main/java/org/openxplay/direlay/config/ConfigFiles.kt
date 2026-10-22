// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-22
package org.openxplay.direlay.config

import android.content.Context
import android.net.Uri
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import org.openxplay.direlay.publicname.PublicNameStore
import org.openxplay.direlay.publicname.PublicNameService
import org.openxplay.direlay.settings.RelaySettingsStore
import org.openxplay.direlay.tls.TlsStore

class ConfigFileResult(val changedCount: Int = 0, val ignoredKeys: List<String> = emptyList(),
    val errors: Map<String, String> = emptyMap()) {
    override fun toString() = "ConfigFileResult(changedCount=$changedCount, errors=${errors.size})"
}

/** The only configuration-file entry point for UI; storage and cryptography stay on IO workers. */
class ConfigFiles(private val context: Context, private val settings: RelaySettingsStore,
    private val publicName: PublicNameService, private val tls: TlsStore) {
    private val names = PublicNameStore(context.noBackupFilesDir)
    private fun meta() = ConfigMeta(context.packageManager.getPackageInfo(context.packageName, 0).versionName.orEmpty())
    suspend fun hasPrivateData(): Boolean = withContext(Dispatchers.IO) {
        names.config() != null || names.savedProviders().isNotEmpty() || tls.hasImported() || tls.acmeMaterial() != null ||
            settings.load().remoteToken.isNotEmpty()
    }
    private fun privacy(): PrivacyConfig {
        val config = names.config()
        val certificate = tls.acmeMaterial()?.let { ConfigCertificate(it.chain, it.key, it.name) }
        val account = config?.let { c -> names.accountKey()?.let { ConfigAccount(it, c.acme.directory, names.accountUrl(c.acme.directory)) } }
        val imported = tls.importedMaterial()?.let { ConfigCertificate(it.first, it.second) }
        return PrivacyConfig(config, certificate, account, names.savedProviders().takeIf { it.isNotEmpty() }, imported,
            settings.load().remoteToken.takeIf { it.isNotEmpty() })
    }
    suspend fun export(uri: Uri, privateData: Boolean): ConfigFileResult = withContext(Dispatchers.IO) {
        try {
            val text = if (privateData) PrivacyConfigCodec.encode(privacy(), meta()) else synchronized(settings) {
                UserConfigCodec.encode(settings.load(), settings.developerOverrides.value, meta())
            }
            val bytes = text.toByteArray(Charsets.UTF_8)
            try { checkNotNull(context.contentResolver.openOutputStream(uri, "wt")).use { it.write(bytes) } }
            finally { bytes.fill(0) }
            ConfigFileResult()
        } catch (e: ConfigFailure) { ConfigFileResult(errors = mapOf(e.key to e.code)) }
        catch (_: Exception) { ConfigFileResult(errors = mapOf("file" to "io")) }
    }
    suspend fun import(uri: Uri, privateData: Boolean): ConfigFileResult = withContext(Dispatchers.IO) {
        var bytes: ByteArray? = null
        try {
            val data = checkNotNull(context.contentResolver.openInputStream(uri)).use { input ->
                val out = java.io.ByteArrayOutputStream()
                val buffer = ByteArray(4096)
                try {
                    while (true) {
                        val count = input.read(buffer)
                        if (count < 0) break
                        if (out.size() + count > ConfigEnvelope.MAX_BYTES) throw ConfigFailure("file", "too-large")
                        out.write(buffer, 0, count)
                    }
                    out.toByteArray()
                } finally { buffer.fill(0) }
            }
            bytes = data
            if (privateData) importPrivacy(data) else synchronized(settings) {
                val current = settings.load()
                val parsed = UserConfigCodec.decode(data, current, settings.developerOverrides.value, settings.wireless(current))
                if (parsed.errors.isEmpty()) {
                    settings.save(parsed.settings, parsed.developer)
                    // The file comes from a configured relay: skip the setup guide on the next start.
                    settings.setupCompleted = true
                }
                ConfigFileResult(parsed.changedCount, parsed.ignoredKeys, parsed.errors)
            }
        } catch (e: ConfigFailure) { ConfigFileResult(errors = mapOf(e.key to e.code)) }
        catch (_: Exception) { ConfigFileResult(errors = mapOf("file" to "io")) }
        finally { bytes?.fill(0) }
    }
    private fun importPrivacy(bytes: ByteArray): ConfigFileResult {
        var result = ConfigFileResult()
        publicName.importConfigurationFiles {
            val parsed = PrivacyConfigCodec.decode(bytes, names.config())
            val value = parsed.value
            if (value == null) result = ConfigFileResult(errors = parsed.errors)
            else {
                // Install the certificate and account before waking the service: no unnecessary ACME order.
                value.publicNameCertificate?.let { tls.installAcme(checkNotNull(it.name), it.chain, it.key) }
                value.importedCertificate?.let { tls.importMaterial(it.chain, it.key) }
                value.publicName?.let(names::save)
                value.providers?.let(names::importProviders)
                value.publicNameAccount?.let { names.importAccount(it.pair, it.directory, it.url) }
                value.mfiRemoteToken?.let { token -> synchronized(settings) {
                    settings.save(settings.load().copy(remoteToken = token), settings.developerOverrides.value)
                } }
                val count = listOf(value.publicName, value.publicNameCertificate, value.publicNameAccount,
                    value.providers, value.importedCertificate, value.mfiRemoteToken).count { it != null }
                result = ConfigFileResult(count)
            }
        }
        return result
    }
}
