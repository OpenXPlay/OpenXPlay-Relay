// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-14
package org.openxplay.direlay

import com.shilapi.xcertplay.airplay.MediaSink
import java.util.concurrent.ConcurrentHashMap
import java.util.concurrent.atomic.AtomicLong

class CountingMediaSink(private val sink: MediaSink) : MediaSink by sink {
    private val counts = ConcurrentHashMap<Int, AtomicLong>()

    val frameCounts: Map<Int, Long>
        get() = counts.mapValues { it.value.get() }

    override fun onVideoFrame(type: Int, naluBytes: ByteArray) {
        countFrame(type)
        sink.onVideoFrame(type, naluBytes)
    }

    override fun onVideoFrame(
        type: Int,
        naluBytes: ByteArray,
        senderNanos: Long,
        arrivalNanos: Long,
    ) {
        countFrame(type)
        sink.onVideoFrame(type, naluBytes, senderNanos, arrivalNanos)
    }

    private fun countFrame(type: Int) {
        counts.computeIfAbsent(type) { AtomicLong() }.incrementAndGet()
    }
}
