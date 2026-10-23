// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-22
package org.openxplay.direlay.miracast

import android.content.Context
import android.graphics.Color
import android.graphics.PixelFormat
import android.hardware.display.DisplayManager
import android.os.Build
import android.os.Handler
import android.os.Looper
import android.util.DisplayMetrics
import android.view.Gravity
import android.view.Surface
import android.view.SurfaceHolder
import android.view.SurfaceView
import android.view.View
import android.view.ViewGroup
import android.view.WindowManager
import android.widget.FrameLayout
import org.openxplay.stream.core.DecoderSupport
import org.openxplay.stream.core.OutputCapabilities

internal interface MiracastDisplays : AutoCloseable {
    fun start(changed: () -> Unit)
    fun displays(): List<MiracastDisplay>
}

internal class AndroidMiracastDisplays(context: Context, private val main: Handler) : MiracastDisplays {
    private val manager = context.getSystemService(DisplayManager::class.java)
    private var changed: () -> Unit = {}
    private val listener = object : DisplayManager.DisplayListener {
        override fun onDisplayAdded(displayId: Int) = changed()
        override fun onDisplayRemoved(displayId: Int) = changed()
        override fun onDisplayChanged(displayId: Int) = changed()
    }
    override fun start(changed: () -> Unit) {
        this.changed = changed
        manager.registerDisplayListener(listener, main)
    }
    override fun displays(): List<MiracastDisplay> = manager.getDisplays(DisplayManager.DISPLAY_CATEGORY_PRESENTATION)
        .filter { it.isValid }.map { display ->
            val mode = display.mode
            val metrics = DisplayMetrics().also { display.getRealMetrics(it) }
            MiracastDisplay(display.displayId, display.name, mode.physicalWidth, mode.physicalHeight,
                metrics.densityDpi, mode.refreshRate.toDouble()).let(MiracastPolicy::normalize)
        }
    override fun close() { manager.unregisterDisplayListener(listener); changed = {} }
}

internal interface MiracastWindow : AutoCloseable {
    /** Zero dimensions hide the SurfaceView so the retained decoder image cannot cover the black window. */
    fun videoSize(width: Int, height: Int)
    fun interface Factory { fun create(display: MiracastDisplay, surface: (Surface?) -> Unit): MiracastWindow }
}

internal class AndroidMiracastWindow(context: Context, private val display: MiracastDisplay,
    private val surface: (Surface?) -> Unit, private val stretch: Boolean = false) : MiracastWindow {
    private val windowContext: Context
    private val manager: WindowManager
    private val root: FrameLayout
    private val video: SurfaceView
    private var width = 0
    private var height = 0
    private var closed = false
    private val callback = object : SurfaceHolder.Callback {
        override fun surfaceCreated(holder: SurfaceHolder) { if (!closed) surface(holder.surface) }
        override fun surfaceChanged(holder: SurfaceHolder, format: Int, width: Int, height: Int) {
            // SurfaceHolder can reuse the Java Surface when its native buffer queue changes.
            if (!closed) { surface(null); surface(holder.surface) }
        }
        override fun surfaceDestroyed(holder: SurfaceHolder) { surface(null) }
    }

    init {
        check(Looper.myLooper() == Looper.getMainLooper())
        val target = checkNotNull(context.getSystemService(DisplayManager::class.java).getDisplay(display.id))
        check(target.isValid) { "display_invalid" }
        windowContext = if (Build.VERSION.SDK_INT >= 30)
            context.createWindowContext(target, WindowManager.LayoutParams.TYPE_APPLICATION_OVERLAY, null)
        else context.createDisplayContext(target)
        manager = windowContext.getSystemService(WindowManager::class.java)
        root = FrameLayout(windowContext).apply { setBackgroundColor(Color.BLACK) }
        video = SurfaceView(windowContext).apply { visibility = View.INVISIBLE; holder.addCallback(callback) }
        root.addView(video, FrameLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.MATCH_PARENT, Gravity.CENTER))
        root.addOnLayoutChangeListener { _, _, _, _, _, _, _, _, _ -> fit() }
        val flags = WindowManager.LayoutParams.FLAG_NOT_FOCUSABLE or WindowManager.LayoutParams.FLAG_NOT_TOUCHABLE or
            WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON or WindowManager.LayoutParams.FLAG_LAYOUT_IN_SCREEN or
            WindowManager.LayoutParams.FLAG_FULLSCREEN
        val params = WindowManager.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.MATCH_PARENT,
            WindowManager.LayoutParams.TYPE_APPLICATION_OVERLAY, flags, PixelFormat.OPAQUE).apply {
            gravity = Gravity.TOP or Gravity.LEFT
            if (Build.VERSION.SDK_INT >= 28) layoutInDisplayCutoutMode = WindowManager.LayoutParams.LAYOUT_IN_DISPLAY_CUTOUT_MODE_SHORT_EDGES
        }
        try { manager.addView(root, params) }
        catch (failure: Exception) {
            closed = true
            video.holder.removeCallback(callback)
            runCatching { manager.removeViewImmediate(root) }
            throw failure
        }
    }

    override fun videoSize(width: Int, height: Int) {
        check(Looper.myLooper() == Looper.getMainLooper())
        this.width = width; this.height = height
        video.visibility = if (width > 0 && height > 0) View.VISIBLE else View.INVISIBLE
        if (width <= 0 || height <= 0) surface(null)
        fit()
    }
    private fun fit() {
        if (closed) return
        val size = MiracastPolicy.fit(root.width.takeIf { it > 0 } ?: display.width,
            root.height.takeIf { it > 0 } ?: display.height, width, height, stretch)
        val params = video.layoutParams as FrameLayout.LayoutParams
        if (params.width != size.width || params.height != size.height) {
            params.width = size.width; params.height = size.height; params.gravity = Gravity.CENTER
            video.layoutParams = params
        }
    }
    override fun close() {
        check(Looper.myLooper() == Looper.getMainLooper())
        if (closed) return
        closed = true
        surface(null)
        video.holder.removeCallback(callback)
        runCatching { manager.removeViewImmediate(root) }
    }
}

/** Main-looper controller; the platform seams also let tests exercise hot-plug and permission recovery. */
internal class MiracastDisplayController(
    private val main: Handler,
    private val source: MiracastDisplays,
    private val windows: MiracastWindow.Factory,
    private val permission: () -> Boolean,
    private val decoders: List<DecoderSupport>,
    private val listener: MiracastListener,
    private val capabilities: (OutputCapabilities) -> Unit,
    private val surface: (Surface?) -> Unit,
    private val picture: MiracastPicture = MiracastPicture(),
) : AutoCloseable {
    private var running = false
    private var display: MiracastDisplay? = null
    private var window: MiracastWindow? = null
    private var epoch = 0L
    private var denied = false
    private var failed = false
    private var state: Pair<MiracastState, MiracastDisplay?>? = null
    private var reported: OutputCapabilities? = null
    private var videoWidth = 0
    private var videoHeight = 0
    private val poll = Runnable { reconcile() }

    fun start() {
        running = true
        source.start(::reconcile)
        reconcile()
    }
    fun videoSize(width: Int, height: Int) {
        videoWidth = width; videoHeight = height
        window?.videoSize(width, height)
    }
    private fun removeWindow() {
        epoch++
        surface(null)
        window?.close(); window = null
    }
    private fun report(value: MiracastState, target: MiracastDisplay?) {
        val next = value to target
        if (state != next) { state = next; listener.state(value, target) }
        val caps = MiracastPolicy.capabilities(decoders, target.takeIf { value == MiracastState.SHOWING }, picture)
        if (reported != caps) { reported = caps; capabilities(caps) }
    }
    private fun reconcile() {
        if (!running) return
        main.removeCallbacks(poll)
        val available = source.displays()
        val next = MiracastPolicy.display(available)
        if (display != next) {
            val previous = display
            removeWindow()
            display = next
            failed = false
            if (previous != null && available.none { it.id == previous.id }) listener.failure("display_lost", "Presentation display removed")
        }
        if (!permission()) {
            removeWindow()
            report(MiracastState.NEEDS_PERMISSION, next)
            if (!denied) { denied = true; listener.failure("overlay_denied", "Display over other apps permission required") }
        } else {
            denied = false
            if (next == null) { report(MiracastState.WAITING_FOR_DISPLAY, null); return }
            if (window == null) {
                val version = ++epoch
                try {
                    window = windows.create(next) { if (running && epoch == version) surface(it) }
                    window?.videoSize(videoWidth, videoHeight)
                    failed = false
                } catch (failure: Exception) {
                    removeWindow()
                    if (!failed) { failed = true; listener.failure("window_failed", failure.javaClass.simpleName) }
                }
            }
            report(if (window != null) MiracastState.SHOWING else MiracastState.WAITING_FOR_DISPLAY,
                next.takeIf { window != null })
        }
        if (running && next != null) main.postDelayed(poll, 2_000)
    }
    override fun close() {
        if (!running) return
        running = false
        main.removeCallbacks(poll)
        source.close()
        removeWindow()
        display = null
        report(MiracastState.ENDED, null)
    }
}
