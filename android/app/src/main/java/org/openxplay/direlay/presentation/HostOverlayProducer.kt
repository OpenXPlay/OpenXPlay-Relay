// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-25
package org.openxplay.direlay.presentation

import android.content.pm.PackageManager
import android.net.VpnService
import android.os.Build
import com.shilapi.xcertplay.AirPlayPersistence
import com.shilapi.xcertplay.orchestration.MfiTarget
import org.openxplay.direlay.R
import org.openxplay.direlay.miracast.MiracastSystem
import org.openxplay.direlay.path.*
import org.openxplay.direlay.relay.*
import org.openxplay.direlay.settings.*
import org.openxplay.direlay.storage.MaterialStatus
import org.openxplay.direlay.ui.*

internal class HostOverlayProducer(private val context: android.content.Context, private val runtime: RelayRuntime) {
    private var wifi: String? = null
    private var name: String? = null
    private var lastRefresh = Long.MIN_VALUE / 2
    private var checked = -1L
    private val version by lazy { context.packageManager.getPackageInfo(context.packageName, 0).versionName.orEmpty() }
    fun sample(revision: Long, resumed: Boolean): HostOverlaySnapshot? {
        val now = android.os.SystemClock.elapsedRealtime()
        if (revision != checked || now - lastRefresh >= 5000) {
            wifi = currentWifiSsid(context)
            runtime.refreshIphoneBonds()
            name = runtime.bondedDevices.localName()
            runtime.autoStartSessions.refresh()
            runtime.recheckNetworkStart()
            lastRefresh = now; checked = revision
        }
        return runtime.overlaySnapshot(context, wifi, name, version)?.also {
            if (resumed && !it.localPreview) runtime.connectionLog.transition("ui", "path", "path_line", it.path.diagnostic)
        }
    }
}

/** Called on IO: Android's bond/name/material reads must never block the CarPlay surface. */
private fun RelayRuntime.overlaySnapshot(activity: android.content.Context, wifi: String?, name: String?, version: String): HostOverlaySnapshot? {
    val settings = settingsStore.snapshot.value ?: return null
    val wireless = settingsStore.wireless(settings)
    val carlink = settings.carlinkOutput
    val required = StartupPermissions.required(Build.VERSION.SDK_INT, wireless, settings.wirelessNetwork.choice.mode,
        AirPlayPersistence.loadLocationReportingEnabled(activity), carlink)
    val identity = settings.mfiTarget != MfiTarget.LOCAL || identityStore.status() == MaterialStatus.Installed
    val valid = runCatching { settings.validate(wireless) }.isSuccess && (!carlink || Build.VERSION.SDK_INT >= 29)
    val input = pathInput(PathLocal(
        permissionsGranted = required.count { activity.checkSelfPermission(it) == PackageManager.PERMISSION_GRANTED },
        permissionsTotal = required.size,
        vpnConsentNeeded = needsVpnConsent(settings) && VpnService.prepare(activity) != null,
        overlayNeeded = settings.miracastOutput && !MiracastSystem.canDrawOverlays(activity),
        identity = if (identity) IdentityState.READY else IdentityState.MISSING,
        identityLabel = if (!identity) null else activity.getString(when (settings.mfiTarget) {
            MfiTarget.LOCAL -> R.string.direlay_installed
            MfiTarget.REMOTE -> R.string.dev_remote
            MfiTarget.I2C -> R.string.dev_linux_i2c
            MfiTarget.USB_CH341 -> R.string.dev_usb_i2c
        }), wifiSsid = wifi, bluetoothName = name,
        setupSummary = modeRows(settings, wireless).joinToString(" · ") { activity.getString(it.value) },
        nowMillis = System.currentTimeMillis(), version = version, setupValid = valid))
    return HostOverlaySnapshot(input.copy(nowMillis = 0), headUnitUrls(), settings.localPreview,
        homeProblems(visibleHomeFailures(settings, status.failures), carlink && carlinkFailed,
            carlinkDetail, carlinkError, serverReason.takeIf { settings.browserOutput },
            serverState, networkReadiness(settings, networkReadings), teslaFailure, carlinkErrorTrail,
            carlinkErrorAt, carPlayDiagnostics.error, dlnaFailure.takeIf { settings.dlnaOutput },
            miracastFailure.takeIf { settings.miracastOutput }), presentPath(input))
}

