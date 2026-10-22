// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-15
package org.openxplay.direlay.ui

/** Entries encode an optional settings section or snapshot timestamp. */
data class RelayBackStack(val entries: List<String> = listOf("home")) {
    val current get() = entries.last()
    val canBack get() = entries.size > 1
    fun open(entry: String): RelayBackStack {
        val route = relayRoute(entry)
        if (route == current) return this
        val parents = relayRouteParents(route)
        if (parents.isEmpty() && route != "settings") return copy(entries = entries + route)
        // Replace the current menu branch while retaining the screen that opened Settings.
        val root = entries.indexOfFirst { it == "settings" || it.startsWith("settings/") || it.startsWith("developer") }
        val prefix = if (root >= 0) entries.take(root) else entries
        val retainedParents = parents.mapIndexed { index, parent ->
            entries.getOrNull(prefix.size + index)?.takeIf { it.substringBefore('#') == parent } ?: parent
        }
        return copy(entries = prefix + retainedParents + route)
    }
    fun back() = if (canBack) copy(entries = entries.dropLast(1)) else this
}
enum class WindowLayout { COMPACT, MEDIUM, EXPANDED }
fun windowLayout(widthDp: Int) = when { widthDp < 600 -> WindowLayout.COMPACT; widthDp < 840 -> WindowLayout.MEDIUM; else -> WindowLayout.EXPANDED }

/** Height is measured from this window, never the physical display or its orientation. */
data class HomeWindow(val layout: WindowLayout, val inlineActions: Boolean)
fun homeWindow(widthDp: Int, heightDp: Int) = HomeWindow(windowLayout(widthDp), heightDp < 480)

/** Old field links resolve to a destination plus an optional card/picker target. */
fun relayRoute(entry: String): String = when {
    entry.startsWith("settings:") -> when (entry.substringAfter(':')) {
        "output" -> "settings/output#output"
        "car" -> "settings/car"
        "picture" -> "settings/picture"
        "audio" -> "settings/audio"
        "connection" -> "settings/connection#connection"
        "iphone" -> "settings/connection#iphone"
        "network" -> "settings/connection/network"
        "sharedWifi" -> "settings/connection/network#sharedWifi"
        "startup" -> "settings/startup"
        "admin", "access" -> "settings/access"
        "publicName" -> "settings/access/publicName"
        "identity" -> "settings/identity"
        "about" -> "settings/about"
        "developer" -> "developer"
        else -> "settings"
    }
    entry == "publicName" -> "settings/access/publicName"
    entry == "about" -> "settings/about"
    entry == "developer/identity" -> "settings/identity"
    else -> entry
}

fun relayRouteParents(route: String): List<String> = when (route.substringBefore('#')) {
    "settings" -> emptyList()
    "settings/connection/network" -> listOf("settings", "settings/connection")
    "settings/access/publicName" -> listOf("settings", "settings/access")
    "developer" -> listOf("settings")
    else -> when {
        route.startsWith("developer/") -> listOf("settings", "developer")
        route.startsWith("settings/") -> listOf("settings")
        else -> emptyList()
    }
}

fun settingsSessionActive(entries: List<String>) = entries.any { it.startsWith("settings") || it.startsWith("developer") }

fun firstSettingsErrorPage(errors: Set<String>) = listOf("settings/output", "settings/connection/network",
    "settings/identity", "developer/video", "developer/connection").firstOrNull { it in errors } ?: "settings"

fun showsDeveloperDiagnostics(route: String) = route in setOf("developer", "developer/diagnostics", "settings/identity")

fun showsIdentityStatus(route: String) = route == "settings"
