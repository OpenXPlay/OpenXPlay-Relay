// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-14
package org.openxplay.direlay

import android.app.Application
import android.app.Activity
import android.app.ActivityOptions
import android.content.Intent
import android.os.Bundle
import android.os.Handler
import android.os.Looper
import android.view.Display
import com.shilapi.xcertplay.AirPlayPersistence
import com.shilapi.xcertplay.airplay.MediaSinkDecorator
import com.shilapi.xcertplay.airplay.MediaSinkDecoratorProvider
import org.openxplay.direlay.admin.AndroidBluetoothVisibility
import org.openxplay.direlay.admin.PairingWindow
import org.openxplay.direlay.bridge.StreamBridgeSink
import org.openxplay.direlay.relay.RelayRuntime
import org.openxplay.direlay.service.AndroidRelayKeepAlive

class DiRelayApplication : Application(), com.shilapi.xcertplay.orchestration.CarPlayEventObserverProvider, MediaSinkDecoratorProvider, com.shilapi.xcertplay.CarPlayCanvasProvider, com.shilapi.xcertplay.CarPlayViewAreaProvider, com.shilapi.xcertplay.CarPlayPhoneProvider, com.shilapi.xcertplay.CarPlayHostOverlayProvider {
    lateinit var runtime: RelayRuntime
        private set
    lateinit var keepAlive: AndroidRelayKeepAlive
        private set

    override val carPlayEventObserver: com.shilapi.xcertplay.orchestration.CarPlayEventObserver?
        get() = if (::runtime.isInitialized) org.openxplay.direlay.relay.CompositeCarPlayObserver(
            runtime.carPlayDiagnostics,
            runtime.pluginContext.takeIf { runtime.settingsStore.snapshot.value?.plugins == true },
        ) else null

    override fun onCreate() {
        super.onCreate()
        installOutOfMemoryDump()
        keepAlive = AndroidRelayKeepAlive(this)
        runtime = RelayRuntime(this, pairing = PairingWindow(AndroidBluetoothVisibility(this, ::showRelayScreen)),
            keepAlive = keepAlive)
        // Log recording must never stop the relay itself.
        if (android.os.Build.VERSION.SDK_INT < 28 || Application.getProcessName() == packageName) runCatching { runtime.logs.start() }
            .onFailure { android.util.Log.e("OXP-Logs", "Log recorder unavailable", it) }
        runtime.carlinkPins.onPending = { showPendingPin() }
        runtime.carlinkPins.onCancel = runtime::cancelCarlinkPairing
        registerActivityLifecycleCallbacks(object : ActivityLifecycleCallbacks {
            override fun onActivityResumed(activity: Activity) { foreground = activity; showPendingPin() }
            override fun onActivityPaused(activity: Activity) { if (foreground === activity) foreground = null }
            override fun onActivityCreated(activity: Activity, state: Bundle?) = Unit
            override fun onActivityStarted(activity: Activity) = Unit
            override fun onActivityStopped(activity: Activity) = Unit
            override fun onActivitySaveInstanceState(activity: Activity, state: Bundle) = Unit
            override fun onActivityDestroyed(activity: Activity) { if (foreground === activity) foreground = null }
        })
    }

    override fun carPlayHostOverlay(activity: androidx.activity.ComponentActivity): android.view.View? =
        org.openxplay.direlay.ui.createCarPlayHostOverlay(activity, runtime)

    override fun carPlayPhoneAddress(): String? {
        val settings = runtime.settingsStore.awaitSnapshot()
        runtime.refreshIphoneBonds()
        return runtime.iphoneSelection(settings.iphoneAddress).address
    }

    private val main = Handler(Looper.getMainLooper())
    @Volatile private var foreground: Activity? = null
    private fun showPendingPin() {
        main.post { if (runtime.carlinkPins.pending != null) bringRelayActivity() }
    }

    /** Any thread: brings the relay screen forward for a Bluetooth pairing prompt; false in the background. */
    private fun showRelayScreen(): Boolean {
        if (foreground == null) return false
        main.post(::bringRelayActivity)
        return true
    }

    private fun bringRelayActivity() {
        val activity = foreground ?: return
        if (activity is RelayActivity || activity.isFinishing) return
        // The DiPlay host may be on a virtual display; pairing always belongs on the relay screen.
        runCatching {
            activity.startActivity(Intent(activity, RelayActivity::class.java)
                .addFlags(Intent.FLAG_ACTIVITY_REORDER_TO_FRONT),
                ActivityOptions.makeBasic().setLaunchDisplayId(Display.DEFAULT_DISPLAY).toBundle())
        }
    }

    /** CarPlay draws at the car's size (Automatic or a fixed profile), not the relay phone's window. */
    override fun carPlayCanvas(): Pair<Int, Int>? = runtime.carPlayScreen?.profile?.let { it.width to it.height }

    override fun carPlayViewAreas(canvasWidth: Int, canvasHeight: Int): List<com.shilapi.xcertplay.airplay.AirPlayViewArea>? =
        runtime.planViewAreas(canvasWidth, canvasHeight)?.map { com.shilapi.xcertplay.airplay.AirPlayViewArea(it.width, it.height) }

    override val mediaSinkDecorator = MediaSinkDecorator { sink, control ->
        runtime.prepareCarPlay()
        // Local preview stays a DiPlay sink, never a ProjectionOutput: its relay-phone display
        // must not participate in Stream's primary-output selection, even when preview is enabled.
        StreamBridgeSink(runtime.input, sink, control,
            playAudioLocally = { runtime.playCarPlayAudio },
            microphoneSource = { runtime.microphoneSource },
            localPreview = { runtime.localPreview },
            fpsHint = AirPlayPersistence.loadFps(this).toDouble(),
            reportInvalid = runtime::reportInvalidVideo,
        )
    }

    override fun onTerminate() {
        if (android.os.Build.VERSION.SDK_INT < 28 || Application.getProcessName() == packageName) runCatching { runtime.logs.recorder.close() }
        runtime.close()
        super.onTerminate()
    }

    /**
     * Diagnostics (2026-10-15): the heap once grew by ~100 MB/s until OutOfMemoryError, too fast to
     * sample. Dump the heap once, at the failure, into the app's external files directory (readable
     * with adb; contains no credentials beyond what the process holds in memory), then crash as before.
     */
    private fun installOutOfMemoryDump() {
        val previous = Thread.getDefaultUncaughtExceptionHandler()
        val dumped = java.util.concurrent.atomic.AtomicBoolean()
        Thread.setDefaultUncaughtExceptionHandler { thread, error ->
            if (generateSequence(error) { it.cause }.any { it is OutOfMemoryError } && dumped.compareAndSet(false, true)) {
                runCatching {
                    val dir = getExternalFilesDir(null) ?: return@runCatching
                    android.os.Debug.dumpHprofData(java.io.File(dir, "oom.hprof").absolutePath)
                }
            }
            previous?.uncaughtException(thread, error)
        }
    }
}
