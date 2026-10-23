// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-22
package org.openxplay.direlay.miracast

import android.content.Context
import android.os.Handler
import android.os.Looper
import org.openxplay.stream.core.*
import java.util.concurrent.atomic.AtomicBoolean

class MiracastOutput internal constructor(
    context: Context,
    private val listener: MiracastListener,
    override val id: String,
    private val discover: () -> List<DecoderSupport>,
    private val codecs: MiracastCodec.Factory,
    private val picture: MiracastPicture = MiracastPicture(),
) : ProjectionOutput {
    constructor(context: Context, listener: MiracastListener = object : MiracastListener {}, id: String = "miracast",
        picture: MiracastPicture = MiracastPicture()) :
        this(context, listener, id, MiracastCodecs::discover, MiracastCodecs.factory, picture)

    private val context = context.applicationContext
    private val main = Handler(Looper.getMainLooper())
    private val gate = Any()
    private var active: Run? = null

    private inner class Run(val host: OutputHost) {
        val running = AtomicBoolean(true)
        val video = MiracastVideo(host, listener, id, codecs)
        private val layoutPending = AtomicBoolean(false)
        private var display: MiracastDisplayController? = null
        fun layout() {
            if (layoutPending.compareAndSet(false, true)) main.post {
                layoutPending.set(false)
                if (running.get()) video.videoSize().let { display?.videoSize(it.width, it.height) }
            }
        }
        fun start() {
            Thread({
                val decoders = runCatching(discover).getOrDefault(emptyList())
                main.post {
                    if (running.get()) {
                        display = MiracastDisplayController(main, AndroidMiracastDisplays(context, main),
                            MiracastWindow.Factory { target, surface -> AndroidMiracastWindow(context, target, surface, picture.stretch) },
                            { MiracastSystem.canDrawOverlays(context) }, decoders, listener,
                            { if (running.get()) host.capabilities(it) }, video::surface, picture).also {
                            video.videoSize().let { size -> it.videoSize(size.width, size.height) }
                            it.start()
                        }
                    }
                }
                video.run()
            }, "miracast-video").apply { isDaemon = true; start() }
        }
        fun close() {
            if (!running.compareAndSet(true, false)) return
            video.close()
            main.post {
                val controller = display
                if (controller == null) listener.state(MiracastState.ENDED, null)
                else controller.close()
                display = null
            }
        }
    }

    override fun open(host: OutputHost) {
        val run = synchronized(gate) {
            check(active == null) { "Miracast output already open" }
            Run(host).also { active = it }
        }
        run.start()
    }
    private fun current(): Run? = synchronized(gate) { active }
    override fun onSession(state: SessionState, config: SessionConfig) {
        current()?.let { it.video.session(state); it.layout() }
    }
    override fun onVideoFormat(screen: ScreenId, format: VideoFormat?) {
        if (screen == ScreenId.MAIN) current()?.let { it.video.format(format); it.layout() }
    }
    override fun onVideoFrame(frame: VideoFrame) { if (frame.screen == ScreenId.MAIN) current()?.video?.offer(frame) }
    override fun onAudioFormat(stream: Int, format: AudioFormat?) = Unit
    override fun onAudioPacket(packet: AudioPacket) = Unit
    override fun onMicrophone(stream: Int, request: MicRequest?) = Unit
    override fun close() {
        val run = synchronized(gate) { active.also { active = null } }
        run?.close()
    }
}
