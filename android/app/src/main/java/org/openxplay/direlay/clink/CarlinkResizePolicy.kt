// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-21
package org.openxplay.direlay.clink

/** Runtime-owned: session state resets on reconnect, but unsuccessful window/view pairs survive. */
internal class CarlinkResizePolicy {
    data class Size(val width: Int, val height: Int) {
        fun matches(other: Size): Boolean =
            kotlin.math.abs(width.toLong() - other.width) <= 2 && kotlin.math.abs(height.toLong() - other.height) <= 2
    }
    data class Resize(val window: Size, val view: Size)

    var enabled: Boolean = false
        set(value) {
            field = value
            if (!value) pending = null
        }
    private var window: Size? = null
    private var view: Size? = null
    private var triggered = false
    private var pending: Resize? = null
    private val unhelpful = mutableSetOf<Resize>()

    fun startSession() {
        window = null
        view = null
        triggered = false
    }

    fun windowSize(width: Int, height: Int) {
        if (width <= 0 || height <= 0) return
        val next = Size(width, height)
        // Only the next session's first window can judge the pending reconnect.
        if (window == null) {
            pending?.let { if (it.window.matches(next)) unhelpful.add(it) }
            pending = null
        }
        window = next
    }

    fun viewSize(width: Int, height: Int) {
        if (width > 0 && height > 0) view = Size(width, height)
    }

    fun reconnect(playing: Boolean): Resize? {
        if (!enabled || !playing || triggered) return null
        val window = window ?: return null
        val view = view ?: return null
        if (window.matches(view) || unhelpful.any { it.window.matches(window) && it.view.matches(view) }) return null
        return Resize(window, view).also {
            triggered = true
            pending = it
        }
    }
}
