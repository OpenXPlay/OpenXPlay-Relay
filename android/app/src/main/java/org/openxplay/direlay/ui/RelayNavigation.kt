// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-15
package org.openxplay.direlay.ui

/** Entries encode an optional settings section or snapshot timestamp. */
data class RelayBackStack(val entries: List<String> = listOf("home")) {
    val current get() = entries.last()
    val canBack get() = entries.size > 1
    fun open(entry: String) = if (entry == current) this else copy(entries = entries + entry)
    fun back() = if (canBack) copy(entries = entries.dropLast(1)) else this
}
enum class WindowLayout { COMPACT, MEDIUM, EXPANDED }
fun windowLayout(widthDp: Int) = when { widthDp < 600 -> WindowLayout.COMPACT; widthDp < 840 -> WindowLayout.MEDIUM; else -> WindowLayout.EXPANDED }

/** Height is measured from this window, never the physical display or its orientation. */
data class HomeWindow(val layout: WindowLayout, val inlineActions: Boolean)
fun homeWindow(widthDp: Int, heightDp: Int) = HomeWindow(windowLayout(widthDp), heightDp < 480)
fun formatPairingCode(code: String) = code.chunked(4).joinToString(" ")
