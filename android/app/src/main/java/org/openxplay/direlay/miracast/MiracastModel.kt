// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-22
package org.openxplay.direlay.miracast

import android.view.Display
import org.openxplay.stream.core.*
import java.util.ArrayDeque
import kotlin.math.roundToInt

data class MiracastPicture(val size: Viewport? = null, val stretch: Boolean = false)

enum class MiracastState { WAITING_FOR_DISPLAY, NEEDS_PERMISSION, SHOWING, ENDED }
data class MiracastDisplay(val id: Int, val name: String, val width: Int, val height: Int, val densityDpi: Int, val refreshHz: Double)
data class MiracastStatistics(val decodedFrames: Long, val droppedFrames: Long, val codec: VideoCodec?, val width: Int, val height: Int)
interface MiracastListener {
    fun state(state: MiracastState, display: MiracastDisplay?) {}
    fun failure(code: String, detail: String) {}
    fun diagnostic(message: String) {}
    fun statistics(statistics: MiracastStatistics) {}
}

internal object MiracastPolicy {
    fun normalize(display: MiracastDisplay): MiracastDisplay = display.copy(
        densityDpi = display.densityDpi.takeIf { it > 0 } ?: 160,
        refreshHz = display.refreshHz.takeIf { it.isFinite() && it > 0 } ?: 60.0,
    )

    fun display(displays: List<MiracastDisplay>): MiracastDisplay? = displays.firstOrNull {
        it.id != Display.DEFAULT_DISPLAY && it.width > 0 && it.height > 0
    }?.let(::normalize)

    fun capabilities(decoders: List<DecoderSupport>, display: MiracastDisplay?, picture: MiracastPicture = MiracastPicture()) = OutputCapabilities(
        latency = LatencyClass.INTERACTIVE, decoders = decoders,
        displays = display?.let(::normalize)?.let { mapOf(ScreenId.MAIN to DisplayInfo(picture.size?.width ?: it.width, picture.size?.height ?: it.height, it.densityDpi, it.refreshHz)) }.orEmpty(),
        touch = 0, keys = emptySet(), audio = emptySet(), microphone = false,
    )

    fun sameFormat(a: VideoFormat?, b: VideoFormat?): Boolean = a === b || a != null && b != null &&
        a.codec == b.codec && a.width == b.width && a.height == b.height && a.config.contentEquals(b.config)

    fun fit(width: Int, height: Int, videoWidth: Int, videoHeight: Int, stretch: Boolean = false): Viewport {
        require(width > 0 && height > 0)
        if (stretch || videoWidth <= 0 || videoHeight <= 0) return Viewport(width, height)
        val scale = minOf(width.toDouble() / videoWidth, height.toDouble() / videoHeight)
        return Viewport((videoWidth * scale).roundToInt().coerceIn(1, width),
            (videoHeight * scale).roundToInt().coerceIn(1, height))
    }
}

/** Caller holds the video gate. Recovery retains only frames starting at the next queued IDR. */
internal class MiracastFrameQueue {
    private val frames = ArrayDeque<VideoFrame>()
    private var waiting = true
    var dropped = 0L
        private set
    val size: Int get() = frames.size
    fun peek(): VideoFrame? = frames.peekFirst()
    fun remove(): VideoFrame? = frames.pollFirst()
    fun clear() { dropped += frames.size; frames.clear(); waiting = true }
    fun recover() {
        while (frames.isNotEmpty() && !frames.first.keyFrame) { frames.removeFirst(); dropped++ }
        waiting = frames.isEmpty()
    }
    fun offer(frame: VideoFrame): Boolean {
        val overflow = frames.size == 8
        if (overflow) { frames.removeFirst(); dropped++; recover() }
        if (waiting && !frame.keyFrame) dropped++
        else { waiting = false; frames.addLast(frame) }
        return overflow
    }
    fun discard() { if (frames.pollFirst() != null) dropped++ }
}
