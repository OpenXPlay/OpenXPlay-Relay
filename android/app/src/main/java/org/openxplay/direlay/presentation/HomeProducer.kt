// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-25
package org.openxplay.direlay.presentation

import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.net.VpnService
import android.os.Build
import com.shilapi.xcertplay.AirPlayPersistence
import com.shilapi.xcertplay.orchestration.MfiTarget
import org.openxplay.direlay.R
import org.openxplay.direlay.path.*
import org.openxplay.direlay.relay.*
import org.openxplay.direlay.settings.*
import org.openxplay.direlay.miracast.MiracastSystem
import org.openxplay.direlay.ui.HomeContent

data class PermissionState(val missing: List<String>, val consent: Intent?, val overlayNeeded: Boolean)
data class HomeSample(val home: HomeContent, val automatic: AutoStartSnapshot, val flags: HostFlags)

internal class HomeProducer(private val context: Context, private val runtime: RelayRuntime,
    private val dispatch: (() -> Unit) -> Unit) {
    private val settingsStore get() = runtime.settingsStore
    private val pathVersion by lazy { context.packageManager.getPackageInfo(context.packageName, 0).versionName.orEmpty() }

    private var lastPin: org.openxplay.direlay.clink.DialogPinProvider.Request? = null
    private var pendingPin: PendingPin? = null
    private fun pin(resumed: Boolean): PendingPin? {
        val request = runtime.carlinkPins.pending
        if (request !== lastPin) {
            lastPin = request
            pendingPin = request?.let { current -> PendingPin(current.headUnitId, current.confirmation, current.incorrect,
                { answer -> dispatch { current.respond(answer) } }, { dispatch { current.cancel() } }) }
        }
        return pendingPin.takeIf { resumed }
    }

    fun permissionState(): PermissionState {
        val settings = settingsStore.snapshot.value ?: return PermissionState(emptyList(), null, false)
        val missing = StartupPermissions.required(Build.VERSION.SDK_INT, settingsStore.wireless(settings),
            AirPlayPersistence.loadWirelessHotspotMode(context), AirPlayPersistence.loadLocationReportingEnabled(context),
            settings.carlinkOutput).filter { context.checkSelfPermission(it) != PackageManager.PERMISSION_GRANTED }
        return PermissionState(missing, if (runtime.needsVpnConsent(settings)) VpnService.prepare(context) else null,
            settings.miracastOutput && !MiracastSystem.canDrawOverlays(context))
    }

    fun sample(flags: HostFlags): HomeSample? {
        val settings = settingsStore.snapshot.value ?: return null
        val wireless = settingsStore.wireless(settings)
        val permissions = permissionState()
        val needsVpn = permissions.consent != null
        val valid = runCatching { settings.validate(wireless) }.isSuccess &&
            (!settings.carlinkOutput || Build.VERSION.SDK_INT >= 29)
        val readyIdentity = settings.mfiTarget != MfiTarget.LOCAL || flags.identityReady
        val phase = runtime.status.session?.phase
        val failures = org.openxplay.direlay.ui.visibleHomeFailures(settings, runtime.status.failures)
        val carPlayError = runtime.carPlayDiagnostics.error
        val carlinkFailed = settings.carlinkOutput && runtime.carlinkFailed
        val webReason = runtime.serverReason.takeIf { settings.browserOutput }
        val network = runtime.networkReadings
        val networkReadiness = runtime.networkReadiness(settings, network)
        val dlnaFailure = runtime.dlnaFailure.takeIf { settings.dlnaOutput }
        val miracastFailure = runtime.miracastFailure.takeIf { settings.miracastOutput }
        val deviceName = flags.deviceName ?: settings.carlinkDeviceName
        val problems = org.openxplay.direlay.ui.homeProblems(failures, carlinkFailed, runtime.carlinkDetail,
            runtime.carlinkError, webReason, runtime.serverState, networkReadiness, runtime.teslaFailure,
            runtime.carlinkErrorTrail, runtime.carlinkErrorAt, carPlayError, dlnaFailure, miracastFailure)
        val modes = org.openxplay.direlay.ui.modeRows(settings, wireless)
        val urls = runtime.headUnitUrls()
        val required = StartupPermissions.required(Build.VERSION.SDK_INT, wireless, settings.wirelessNetwork.choice.mode,
            AirPlayPersistence.loadLocationReportingEnabled(context), settings.carlinkOutput)
        val path = runtime.pathInput(org.openxplay.direlay.relay.PathLocal(
            permissionsGranted = required.count { context.checkSelfPermission(it) == PackageManager.PERMISSION_GRANTED },
            permissionsTotal = required.size, vpnConsentNeeded = needsVpn, overlayNeeded = permissions.overlayNeeded,
            identity = when { readyIdentity -> org.openxplay.direlay.path.IdentityState.READY
                !flags.materialChecked -> org.openxplay.direlay.path.IdentityState.CHECKING
                else -> org.openxplay.direlay.path.IdentityState.MISSING },
            identityLabel = if (!readyIdentity) null else context.getString(when (settings.mfiTarget) {
                MfiTarget.LOCAL -> R.string.direlay_installed
                MfiTarget.REMOTE -> R.string.dev_remote
                MfiTarget.I2C -> R.string.dev_linux_i2c
                MfiTarget.USB_CH341 -> R.string.dev_usb_i2c
            }), wifiSsid = flags.wifi, bluetoothName = flags.deviceName,
            setupSummary = modes.joinToString(" · ") { context.getString(it.value) },
            nowMillis = System.currentTimeMillis(), version = pathVersion, setupValid = valid))
        val presentation = presentPath(path)
        if (flags.resumed) runtime.connectionLog.transition("ui", "path", "path_line", presentation.diagnostic)
        val home = org.openxplay.direlay.ui.HomeContent(org.openxplay.direlay.ui.setupHomePresentation(path, presentation, modes), urls,
            deviceName, settings.autoStartWhenReady, settings.carlinkOutput,
            pin(flags.resumed), runtime.startRequest.pending || flags.requestingPermissions, vehicle = runtime.vehicleView,
            pinRejected = runtime.pinRejected, pairingSuccess = runtime.pairingSuccess,
            displayMismatch = runtime.displayMismatch(presentation.running, settings), currentWifi = flags.wifi,
            pathInput = path, path = presentation, networkReadings = network, stepProblems = problems,
            highPerformance = settings.highPerformance)
        return HomeSample(publishHome(path, presentation, home), AutoStartSnapshot(networkReadiness.problems.isEmpty(), runtime.networkStart.pending != null,
            presentation.canStart, runtime.sessionActions.restarting, runtime.startRequest.pending, presentation.running,
            runtime.status.failure != null, runtime.autoStartGate.paused, runtime.autoStartSessions.tracker.observed,
            runtime.autoStartSessions.tracker.running, phase, settings.autoStartWhenReady), flags)
    }
}
