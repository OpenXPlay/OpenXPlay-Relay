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
    val carFirst = CarFirstPolicy()
    val carlinkFirst = CarlinkStartGate()
    private val recovery = RecoverOncePolicy()
    private var userStopped = false
    private var pendingManual = true
    private var carWaitSatisfied = false
    @Volatile override var bootStart = false
    /** Every wait before CarPlay starts, logged on entry, while it lasts and at its end. */
    private val waits = org.openxplay.direlay.logs.WaitLog(android.os.SystemClock::elapsedRealtime) { event, detail ->
        runtime.connectionLog.event("relay", event, detail)
    }
    val waitSummary: String get() = waits.summary()
    @Synchronized override fun action(name: String, manual: Boolean): String? = when (name) {
        "start" -> start(manual)
        "stop" -> {
            endWaits("stopped")
            userStopped = true
            runtime.carlinkPictureStopped()
            networkStart.cancel()
            carFirst.cancel()
            carlinkFirst.cancel()
            runtime.startRequest.finish()
            restarting = false
            generation++
            runtime.autoStartSessions.pause("Stop requested")
            stop(if (manual) "manual" else "auto"); null
        }
        "startNow" -> {
            if (carFirst.untilMillis == null && carlinkFirst.untilMillis == null) "error.conflict"
            else { waits.end("car_screen", "start_now"); waits.end("carlink", "start_now"); carFirst.cancel(); carlinkFirst.cancel(); null }
        }
        "restart", "retry" -> {
            if (runtime.startRequest.pending || restarting || networkStart.pending != null) refuse("error.conflict", "busy",
                "action=$name ${busyDetail()}") else {
                if (name == "retry") {
                    userStopped = false
                    recovery.reset()
                    runtime.status.clearFailure()
                    runtime.carPlayDiagnostics.clear()
                    runtime.iphoneLink.reset()
                    runtime.autoStartSessions.resume("Retry requested")
                    runtime.connectionLog.event("relay", "carplay_retry", "source=${if (manual) "manual" else "auto"}")
                }
                carWaitSatisfied = false
                runtime.displayRestarted()
                runtime.autoStartSessions.tracker.restarting()
                restarting = true
                manualRestart = manual
                stop(if (manual) "manual" else "auto")
                waits.begin("restart", "action=$name")
                awaitStopped(System.nanoTime() + TimeUnit.SECONDS.toNanos(15))
                null
            }
        }
        else -> "error.invalid"
    }
    private fun stop(source: String) {
        runtime.carPlayStage.reset()
        runtime.iphoneLink.reset()
        runtime.connectionLog.event("relay", "carplay_stop", "source=$source")
        runtime.stopTeslaAddress()
        runtime.input.expectStop()
        context.startService(Intent(context, DiPlaySessionService::class.java).setAction(DiPlaySessionService.ACTION_STOP))
    }
    private fun awaitStopped(deadline: Long) {
        worker.schedule({ synchronized(this) {
            if (closed || !restarting) return@schedule
            runtime.autoStartSessions.refresh()
            if (!runtime.autoStartSessions.tracker.running) {
                restarting = false
                waits.end("restart", "stopped")
                startChecked(manualRestart)
            } else if (System.nanoTime() < deadline) { waits.tick("restart"); awaitStopped(deadline) }
            else {
                waits.end("restart", "timeout")
                restarting = false; runtime.carlinkPictureStopped(); runtime.status.inputFailed(IllegalStateException("Restart timed out"))
            }
        } }, 250, TimeUnit.MILLISECONDS)
    }
    @Synchronized override fun start(manual: Boolean): String? {
        if (networkStart.pending != null) {
            // A repeated Start while the radios hold the first one: say again what it waits for.
            waits.tick("network", networkDetail())
            return refuse("error.conflict", "network_wait", "manual=$manual")
        }
        if (runtime.startRequest.pending || restarting) return refuse("error.conflict", "busy", "manual=$manual ${busyDetail()}")
        userStopped = false
        recovery.reset()
        carWaitSatisfied = false
        runtime.autoStartSessions.refresh()
        if (runtime.autoStartSessions.tracker.running) return refuse(null, "session_running", "manual=$manual")
        runtime.carPlayStage.reset()
        runtime.iphoneLink.reset()
        runtime.status.clearFailure()
        runtime.carPlayDiagnostics.clear()
        return startChecked(manual)
    }

    @Synchronized fun cancelNetworkStart() {
        waits.end("network", "cancelled")
        networkStart.cancel()
        userStopped = true
        runtime.autoStartSessions.pause("Start cancelled")
    }

    @Synchronized fun observeNetworkSession(running: Boolean, active: Boolean) = networkStart.observeSession(running, active)

    @Synchronized private fun startFailed(failure: Throwable) {
        endWaits("failed exception=${failure.javaClass.simpleName}")
        carFirst.cancel()
        carlinkFirst.cancel()
        runtime.startRequest.finish()
        networkStart.cancel()
        runtime.status.inputFailed(failure)
    }

    @Synchronized fun startAnyway(): String? {
        if (!networkStart.accept()) return refuse("error.conflict", "no_network_wait")
        waits.end("network", "start_anyway ${networkDetail()}")
        return startAfterNetworkAccepted(skipNetwork = true)
    }

    private fun startAfterNetworkAccepted(skipNetwork: Boolean = false): String? = startChecked(manual = pendingManual, networkAccepted = skipNetwork).also {
        if (it != null) networkStart.cancel()
    }

    /** Called on the runtime tick and after returning from Settings. */
    @Synchronized fun recheckNetworkStart(): String? {
        if (networkStart.pending == null) { waits.end("network", "cleared"); return null }
        val settings = runtime.settingsStore.awaitSnapshot()
        val readiness = runtime.networkReadiness(settings, runtime.refreshNetworkReadings())
        if (!networkStart.recheck(readiness)) { waits.tick("network", networkDetail(readiness)); return null }
        waits.end("network", "ready")
        return startAfterNetworkAccepted()
    }

    @Synchronized fun recoverRadio(readings: org.openxplay.direlay.network.NetworkReadings) {
        val code = recovery.observe(runtime.iphoneLink.snapshot.failure, readings, userStopped) ?: return
        runtime.connectionLog.event("relay", "carplay_radio_recover", "code=$code")
        // Keep the once-only budget across this deliberate restart.
        if (runtime.startRequest.pending || restarting) return
        runtime.autoStartSessions.resume("Radio restored")
        runtime.autoStartSessions.tracker.restarting()
        restarting = true
        carWaitSatisfied = false
        manualRestart = false
        stop("radio")
        waits.begin("restart", "action=radio code=$code")
        awaitStopped(System.nanoTime() + TimeUnit.SECONDS.toNanos(15))
    }

    private fun startChecked(manual: Boolean, networkAccepted: Boolean = false): String? {
        if (closed || runtime.startRequest.pending) return "error.conflict"
        if (manual) runtime.autoStartSessions.resume("Start requested")
        runtime.autoStartSessions.refresh()
        if (runtime.autoStartSessions.tracker.running) return null
        val store = runtime.settingsStore
        val settings = store.awaitSnapshot()
        if (store.wifiDirectConflict(settings)) {
            val carlink = CarlinkConnectionPolicy.enabled(settings)
            runtime.status.inputFailed(IllegalStateException(if (carlink) CarlinkConnectionPolicy.REASON else CarlinkConnectionPolicy.MIRACAST_REASON))
            return refuse(if (carlink) CarlinkConnectionPolicy.REASON_KEY else CarlinkConnectionPolicy.MIRACAST_REASON_KEY, "wifi_direct_conflict")
        }
        val wireless = store.wireless(settings)
        val permissions = StartupPermissions.required(Build.VERSION.SDK_INT, wireless,
            settings.wirelessNetwork.choice.mode, AirPlayPersistence.loadLocationReportingEnabled(context), settings.carlinkOutput)
        val missing = permissions.filter { context.checkSelfPermission(it) != PackageManager.PERMISSION_GRANTED }
        if (missing.isNotEmpty()) return refuse("error.conflict", "permissions", "missing=${missing.joinToString(",") { it.substringAfterLast('.') }}")
        if (runtime.needsVpnConsent(settings, wireless) && VpnService.prepare(context) != null) return refuse("error.conflict", "vpn_consent")
        if (settings.miracastOutput && !org.openxplay.direlay.miracast.MiracastSystem.canDrawOverlays(context)) return refuse("error.conflict", "overlay")
        if (settings.mfiTarget == MfiTarget.LOCAL && runtime.identityStore.status() != MaterialStatus.Installed) return refuse("error.conflict", "identity")
        runCatching { settings.validate(wireless) }.onFailure {
            return refuse("error.invalid", "invalid_settings", "exception=${it.javaClass.simpleName}")
        }
        if (!networkAccepted && !networkStart.request(manual,
                runtime.networkReadiness(settings, runtime.refreshNetworkReadings()))) {
            waits.begin("network", "source=${if (manual) "manual" else "auto"} ${networkDetail()}")
            pendingManual = manual
            if (manual) main.post {
                try {
                    context.startActivity(org.openxplay.direlay.RelayActivity.intent(context))
                } catch (failure: RuntimeException) { startFailed(failure) }
            }
            return null
        }
        runtime.refreshIphoneBonds()
        val pick = iphonePick(runtime.iphoneSelection(settings.iphoneAddress))
        if (wireless && pick != org.openxplay.direlay.path.IphonePick.PICKED) return refuse("error.conflict", "iphone", "pick=${pick.name.lowercase()}")
        if (wireless && settings.wirelessNetwork.choice == org.openxplay.direlay.settings.WirelessNetworkChoice.EXISTING_WIFI &&
            runtime.currentSharedWifi(settings) == null) return refuse("error.sharedWifi", "shared_wifi")
        if (!runtime.startRequest.begin()) return refuse("error.conflict", "busy", busyDetail())
        val source = if (manual) "manual" else if (bootStart) "boot" else "auto"
        bootStart = false
        val launchGeneration = ++generation
        // A transition on the refusal key, so the same refusal after this start is logged again.
        runtime.connectionLog.transition("relay", "start", "carplay_start_requested", "source=$source launch=$launchGeneration")
        runtime.carPlayStage.reset()
        runtime.iphoneLink.reset()
        runtime.status.clearFailure()
        runtime.carPlayDiagnostics.clear()
        carlinkFirst.request(System.currentTimeMillis(), wireless && CarlinkConnectionPolicy.enabled(settings) &&
            settings.experimental.effective().waitForCarlink)
        if (carlinkFirst.untilMillis != null) waits.begin("carlink", "max=${CarlinkStartGate.MAX_WAIT_MILLIS} carlink=${runtime.carlinkState}")
        if (!carWaitSatisfied && carFirst.request(System.currentTimeMillis(), settings.displayMode == org.openxplay.direlay.settings.DisplayMode.AUTOMATIC,
            settings.browserOutput, settings.carlinkOutput,
            runtime.carScreenReported, runtime.savedCarScreen(settings) != null)) waits.begin("car_screen", "max=${CarFirstPolicy.MAX_WAIT_MILLIS}")
        continueStart(launchGeneration, source, networkAccepted)
        return null
    }

    @Synchronized private fun continueStart(launchGeneration: Long, source: String, networkAccepted: Boolean) {
        if (closed || launchGeneration != generation) return
        try {
            val settings = runtime.settingsStore.awaitSnapshot()
            if (!carlinkFirst.ready(System.currentTimeMillis(),
                    runtime.carlinkReadyForCarPlay,
                    CarlinkConnectionPolicy.enabled(settings))) {
                waits.tick("carlink", "carlink=${runtime.carlinkState}")
                worker.schedule({ continueStart(launchGeneration, source, networkAccepted) }, 100, TimeUnit.MILLISECONDS)
                return
            }
            waits.end("carlink", "ready")
        } catch (failure: Exception) { waits.end("carlink", "failed carlink=${runtime.carlinkState}"); startFailed(failure); return }
        if (!carFirst.ready(System.currentTimeMillis(), runtime.carScreenReported)) {
            waits.tick("car_screen")
            worker.schedule({ continueStart(launchGeneration, source, networkAccepted) }, 100, TimeUnit.MILLISECONDS)
            return
        }
        waits.end("car_screen", if (runtime.carScreenReported) "reported" else "timeout")
        carWaitSatisfied = true
        try {
            val store = runtime.settingsStore
            val settings = store.awaitSnapshot()
            val wireless = store.wireless(settings)
            val sharedWifi = if (wireless && settings.wirelessNetwork.choice == org.openxplay.direlay.settings.WirelessNetworkChoice.EXISTING_WIFI)
                runtime.currentSharedWifi(settings) else null
            // Re-read radios and settings after the car wait, before freezing the canvas and codec.
            if (!networkAccepted && !networkStart.request(source == "manual", runtime.networkReadiness(settings, runtime.refreshNetworkReadings()))) {
                waits.begin("network", "source=$source after_car_wait ${networkDetail()}")
                pendingManual = source == "manual"
                runtime.startRequest.finish()
                return
            }
            check(!wireless || settings.wirelessNetwork.choice != org.openxplay.direlay.settings.WirelessNetworkChoice.EXISTING_WIFI || sharedWifi != null) { "Join a saved Wi-Fi" }
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
                finally { if (launchGeneration == generation) runtime.startRequest.finish() }
            }
        } catch (failure: Exception) {
            runtime.startRequest.finish(); startFailed(failure)
        }
    }
    /** Logs why a start request did not go ahead; repeated identical refusals log once. */
    private fun refuse(result: String?, reason: String, detail: String = ""): String? {
        runtime.connectionLog.transition("relay", "start", "carplay_start_blocked", "reason=$reason result=${result ?: "none"} $detail".trimEnd())
        return result
    }
    private fun busyDetail() = "start_pending=${runtime.startRequest.pending} restarting=$restarting network_wait=${networkStart.pending != null}"
    private fun networkDetail(readiness: org.openxplay.direlay.network.NetworkReadiness? = networkStart.pending): String {
        val readings = runtime.networkReadings
        fun on(value: Boolean?) = when (value) { true -> "on"; false -> "off"; null -> "unknown" }
        return "problems=${readiness?.problems.orEmpty().joinToString(",") { it.name.lowercase() }.ifEmpty { "none" }} " +
            "unknown=${readiness?.unknown.orEmpty().joinToString(",") { it.name.lowercase() }.ifEmpty { "none" }} " +
            "wifi=${on(readings.wifi)} hotspot=${on(readings.hotspot)} bluetooth=${on(readings.bluetooth)} " +
            "radios=${runtime.radioConcurrency.name.lowercase()}" + (runtime.radioConflict.value?.let { " conflict=${it.kind.name.lowercase()}" } ?: "")
    }
    private fun endWaits(outcome: String) { listOf("network", "carlink", "car_screen", "restart").forEach { waits.end(it, outcome) } }
    @Synchronized override fun close() { closed = true; generation++; restarting = false; networkStart.cancel(); carFirst.cancel(); carlinkFirst.cancel(); runtime.startRequest.finish(); worker.shutdownNow() }
}
