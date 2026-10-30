// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-25
package org.openxplay.direlay.presentation

import org.openxplay.direlay.path.*
import org.openxplay.direlay.relay.*
import org.openxplay.direlay.settings.*
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
    val carlink = settings.carlinkOutput
    val input = sampledPathInput(activity, settings, wifi, name, version)
    val path = presentPath(input)
    return HostOverlaySnapshot(input.copy(nowMillis = 0), headUnitUrls(), settings.localPreview,
        homeProblems(visibleHomeFailures(settings, status.failures), carlink && carlinkFailed,
            carlinkDetail, carlinkError, serverReason.takeIf { settings.browserOutput },
            serverState, networkReadiness(settings, networkReadings), teslaFailure, carlinkErrorTrail,
            carlinkErrorAt, carPlayDiagnostics.error, dlnaFailure.takeIf { settings.dlnaOutput },
            miracastFailure.takeIf { settings.miracastOutput }), path,
        displayChanged = displayMismatch(path.running, settings), carIncomplete = homeCarRowVisible(settings.car),
        setupCompleted = settingsStore.setupCompleted, highPerformance = settings.highPerformance)
}

