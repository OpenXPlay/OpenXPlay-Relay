// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-22
package org.openxplay.direlay.logs

import android.content.Context
import com.shilapi.xcertplay.AirPlayPersistence
import org.openxplay.direlay.publicname.PublicNameStore
import org.openxplay.direlay.settings.RelaySettings
import org.openxplay.direlay.settings.RelaySettingsStore
import org.openxplay.direlay.tls.TlsStore
import java.io.File

class RelayLogSecrets(private val context: Context, private val settings: RelaySettingsStore, private val tls: TlsStore,
    hook: (String) -> String) {
    val secrets = LogSecrets(::refresh, hook, LogTokens.loadKey(File(context.noBackupFilesDir, "log-token.key")))
    private val names = PublicNameStore(context.noBackupFilesDir)
    private var snapshot: RelaySettings? = null
    private var initialized = false
    private var files: List<String> = emptyList()
    private var checkedFiles = 0L
    @Synchronized private fun refresh(target: LogSecrets) {
        val current = settings.snapshot.value
        if (!initialized || snapshot !== current) {
            target.remember(buildList {
                current?.let {
                    add(it.remoteToken); add(it.wirelessNetwork.manualPassphrase)
                    addAll(it.wirelessNetwork.sharedWifiNetworks.map { network -> network.passphrase })
                }
                add(AirPlayPersistence.loadManualHotspotPassphrase(context))
                add(AirPlayPersistence.loadExistingWifiPassphrase(context))
                add(AirPlayPersistence.loadRemoteMfiToken(context))
            })
            snapshot = current
        }
        // Called for every log line: stat the token files at most every 5 s.
        val now = System.currentTimeMillis()
        if (initialized && now - checkedFiles in 0 until 5_000) return
        checkedFiles = now
        val revision = listOf("public-name/config.json", "public-name/providers.json", "tls/password", "tls-generated/password").map {
            val file = File(context.noBackupFilesDir, it)
            "${file.canonicalPath}:${file.lastModified()}:${file.length()}"
        }
        if (!initialized || files != revision) {
            target.remember(buildList {
                addAll(names.savedProviders().values.map { it.token })
                names.config()?.token?.let(::add)
                addAll(tls.logPasswords())
            })
            files = revision
        }
        initialized = true
    }
}
