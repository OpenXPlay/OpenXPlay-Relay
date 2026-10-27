// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-27
package org.openxplay.direlay.presentation

import android.content.Context
import android.content.pm.PackageManager
import android.net.VpnService
import android.os.Build
import com.shilapi.xcertplay.AirPlayPersistence
import com.shilapi.xcertplay.orchestration.MfiTarget
import org.openxplay.direlay.R
import org.openxplay.direlay.miracast.MiracastSystem
import org.openxplay.direlay.path.IdentityState
import org.openxplay.direlay.path.PathInput
import org.openxplay.direlay.relay.PathLocal
import org.openxplay.direlay.relay.RelayRuntime
import org.openxplay.direlay.relay.pathInput
import org.openxplay.direlay.settings.RelaySettings
import org.openxplay.direlay.settings.StartupPermissions
import org.openxplay.direlay.storage.MaterialStatus
import org.openxplay.direlay.ui.modeRows

/**
 * Samples the Android-only facts (permissions, VPN consent, overlay, identity) and builds the runtime's path input.
 * Called off the main thread: the identity and permission reads may block. Used by the running page and the admin page.
 */
internal fun RelayRuntime.sampledPathInput(context: Context, settings: RelaySettings, wifi: String?, name: String?,
    version: String): PathInput {
    val wireless = settingsStore.wireless(settings)
    val carlink = settings.carlinkOutput
    val required = StartupPermissions.required(Build.VERSION.SDK_INT, wireless, settings.wirelessNetwork.choice.mode,
        AirPlayPersistence.loadLocationReportingEnabled(context), carlink)
    val identity = settings.mfiTarget != MfiTarget.LOCAL || identityStore.status() == MaterialStatus.Installed
    val valid = runCatching { settings.validate(wireless) }.isSuccess && (!carlink || Build.VERSION.SDK_INT >= 29)
    return pathInput(PathLocal(
        permissionsGranted = required.count { context.checkSelfPermission(it) == PackageManager.PERMISSION_GRANTED },
        permissionsTotal = required.size,
        vpnConsentNeeded = needsVpnConsent(settings) && VpnService.prepare(context) != null,
        overlayNeeded = settings.miracastOutput && !MiracastSystem.canDrawOverlays(context),
        identity = if (identity) IdentityState.READY else IdentityState.MISSING,
        identityLabel = if (!identity) null else context.getString(when (settings.mfiTarget) {
            MfiTarget.LOCAL -> R.string.direlay_installed
            MfiTarget.REMOTE -> R.string.dev_remote
            MfiTarget.I2C -> R.string.dev_linux_i2c
            MfiTarget.USB_CH341 -> R.string.dev_usb_i2c
        }), wifiSsid = wifi, bluetoothName = name,
        setupSummary = modeRows(settings, wireless).joinToString(" · ") { context.getString(it.value) },
        nowMillis = System.currentTimeMillis(), version = version, setupValid = valid))
}
