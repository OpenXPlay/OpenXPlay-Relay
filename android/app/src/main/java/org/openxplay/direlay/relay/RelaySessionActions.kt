// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-15
package org.openxplay.direlay.relay

import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.net.VpnService
import android.os.Build
import android.os.Handler
import android.os.Looper
import com.shilapi.xcertplay.AirPlayPersistence
import com.shilapi.xcertplay.CarPlayHostActivity
import com.shilapi.xcertplay.DiPlaySessionService
import com.shilapi.xcertplay.orchestration.MfiTarget
import org.openxplay.direlay.settings.StartupPermissions
import org.openxplay.direlay.settings.CarlinkConnectionPolicy
import org.openxplay.direlay.network.ManualNetworkStart
import org.openxplay.direlay.storage.MaterialStatus
import java.util.concurrent.Executors
import java.util.concurrent.TimeUnit

interface RelayActions {
    val restarting: Boolean
    fun action(name: String, manual: Boolean = true): String?
    fun start(manual: Boolean = false): String?
    /** Set before an automatic start that the boot path triggered; only the timeline reads it. */
    var bootStart: Boolean
}
/** Shared command path for local buttons and browser actions; Android consent remains local. */
class RelaySessionActions(private val context: Context, private val runtime: RelayRuntime) : RelayActions, AutoCloseable {
    private val main = Handler(Looper.getMainLooper())
    private val worker = Executors.newSingleThreadScheduledExecutor { task -> Thread(task, "direlay-session-actions").apply { isDaemon = true } }
    @Volatile override var restarting = false
        private set
    @Volatile private var closed = false
    @Volatile private var generation = 0L
    val networkStart = ManualNetworkStart()
    private var manualRestart = false
    @Volatile override var bootStart = false
    @Synchronized override fun action(name: String, manual: Boolean): String? = when (name) {
        "start" -> start(manual)
        "stop" -> {
            networkStart.cancel()
            restarting = false
            generation++
            runtime.autoStartSessions.pause("Stop requested")
            stop(if (manual) "manual" else "auto"); null
        }
        "restart" -> {
            if (runtime.startRequest.pending || restarting || networkStart.pending != null) "error.conflict" else {
                runtime.displayRestarted()
                runtime.autoStartSessions.tracker.restarting()
                restarting = true
                manualRestart = manual
                stop(if (manual) "manual" else "auto")
                awaitStopped(System.nanoTime() + TimeUnit.SECONDS.toNanos(15))
                null
            }
        }
        else -> "error.invalid"
    }
    private fun stop(source: String) {
        runtime.connectionLog.event("relay", "carplay_stop", "source=$source")
        runtime.stopTeslaAddress()
        runtime.input.expectStop()
        context.startService(Intent(context, DiPlaySessionService::class.java).setAction(DiPlaySessionService.ACTION_STOP))
    }
    private fun awaitStopped(deadline: Long) {
        worker.schedule({
            if (closed || !restarting) return@schedule
            runtime.autoStartSessions.refresh()
            if (!runtime.autoStartSessions.tracker.running) {
                restarting = false
                start(manualRestart)
            } else if (System.nanoTime() < deadline) awaitStopped(deadline)
            else { restarting = false; runtime.status.inputFailed(IllegalStateException("Restart timed out")) }
        }, 250, TimeUnit.MILLISECONDS)
    }
    @Synchronized override fun start(manual: Boolean): String? {
        if (networkStart.pending != null) return "error.conflict"
        return startChecked(manual)
    }

    @Synchronized fun cancelNetworkStart() {
        networkStart.cancel()
        runtime.autoStartSessions.pause("Start cancelled")
    }

    @Synchronized fun observeNetworkSession(running: Boolean, active: Boolean) = networkStart.observeSession(running, active)

    @Synchronized private fun startFailed(failure: Throwable) {
        networkStart.cancel()
        runtime.status.inputFailed(failure)
    }

    @Synchronized fun startAnyway(): String? {
        if (!networkStart.accept()) return "error.conflict"
        return startAfterNetworkAccepted()
    }

    private fun startAfterNetworkAccepted(): String? = startChecked(manual = true, networkAccepted = true).also {
        if (it != null) networkStart.cancel()
    }

    /** Called only while the relay screen is resumed, including after returning from Settings. */
    @Synchronized fun recheckNetworkStart(): String? {
        if (networkStart.pending == null) return null
        val settings = runtime.settingsStore.awaitSnapshot()
        val readiness = runtime.networkReadiness(settings, runtime.refreshNetworkReadings())
        return if (networkStart.recheck(readiness)) startAfterNetworkAccepted() else null
    }

    private fun startChecked(manual: Boolean, networkAccepted: Boolean = false): String? {
        if (closed || runtime.startRequest.pending) return "error.conflict"
        if (manual) runtime.autoStartSessions.resume("Start requested")
        runtime.autoStartSessions.refresh()
        if (runtime.autoStartSessions.tracker.running) return null
        val store = runtime.settingsStore
        val settings = store.awaitSnapshot()
        if (store.carlinkWifiDirectConflict(settings)) {
            runtime.status.inputFailed(IllegalStateException(CarlinkConnectionPolicy.REASON))
            return CarlinkConnectionPolicy.REASON_KEY
        }
        runtime.refreshIphoneBonds()
        val wireless = store.wireless(settings)
        if (wireless && runtime.iphoneSelection(settings.iphoneAddress).address == null) return "error.conflict"
        val permissions = StartupPermissions.required(Build.VERSION.SDK_INT, wireless,
            settings.wirelessNetwork.choice.mode, AirPlayPersistence.loadLocationReportingEnabled(context), settings.carlinkOutput || settings.bothOutputs)
        if (permissions.any { context.checkSelfPermission(it) != PackageManager.PERMISSION_GRANTED }) return "error.conflict"
        if (runtime.needsVpnConsent(settings, wireless) && VpnService.prepare(context) != null) return "error.conflict"
        if (settings.mfiTarget == MfiTarget.LOCAL && runtime.identityStore.status() != MaterialStatus.Installed) return "error.conflict"
        if (runCatching { settings.validate(wireless) }.isFailure) return "error.invalid"
        val sharedWifi = if (wireless && settings.wirelessNetwork.choice == org.openxplay.direlay.settings.WirelessNetworkChoice.EXISTING_WIFI)
            runtime.currentSharedWifi(settings) ?: return "error.sharedWifi" else null
        if (!networkAccepted && !networkStart.request(manual,
                runtime.networkReadiness(settings, runtime.refreshNetworkReadings()))) {
            main.post {
                try {
                    context.startActivity(Intent(context, org.openxplay.direlay.RelayActivity::class.java)
                        .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_REORDER_TO_FRONT))
                } catch (failure: RuntimeException) { startFailed(failure) }
            }
            return null
        }
        if (!runtime.startRequest.begin()) return "error.conflict"
        val source = if (manual) "manual" else if (bootStart) "boot" else "auto"
        bootStart = false
        val launchGeneration = ++generation
        try {
            runtime.status.clearFailure()
            runtime.carPlayDiagnostics.clear()
            sharedWifi?.let { AirPlayPersistence.saveExistingWifiCredentials(context, it.ssid, it.passphrase) }
            store.prepareStart()
            runtime.prepareCarPlay(chooseVideoCodec = true)
            runtime.prepareCarPlayScreen(settings)
            runtime.prepareCarPlayFrameRate()
            runtime.prepareTeslaAddress(settings, wireless)
            main.post {
                try {
                    if (!closed && launchGeneration == generation) {
                        runtime.connectionLog.event("relay", "carplay_start", "source=$source")
                        context.startActivity(Intent(context, CarPlayHostActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
                    }
                } catch (failure: RuntimeException) { startFailed(failure) }
                finally { runtime.startRequest.finish() }
            }
        } catch (failure: Exception) {
            runtime.startRequest.finish(); startFailed(failure); return "error.conflict"
        }
        return null
    }
    @Synchronized override fun close() { closed = true; generation++; restarting = false; networkStart.cancel(); worker.shutdownNow() }
}
