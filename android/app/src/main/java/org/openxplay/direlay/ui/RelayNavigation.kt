// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-15
package org.openxplay.direlay.ui

/** Restored entries obey the same root and page identity rules as new navigation. */
class RelayBackStack(entries: List<String> = listOf("home")) {
    val entries: List<String> = buildList {
        add("home")
        entries.forEach { entry ->
            val route = relayNavigationRoute(entry)
            val index = indexOfFirst { relayPageKey(it) == relayPageKey(route) }
            if (index >= 0) {
                while (size > index) removeAt(lastIndex)
            }
            add(route)
        }
    }
    val current get() = entries.last()
    val canBack get() = entries.size > 1
    fun open(entry: String): RelayBackStack {
        val route = relayNavigationRoute(entry)
        val index = entries.indexOfFirst { relayPageKey(it) == relayPageKey(route) }
        if (index >= 0) return RelayBackStack(entries.take(index) + route)
        val parents = relayRouteParents(route)
        if (parents.isEmpty() && route != "settings") return RelayBackStack(entries + route)
        // Replace the current menu branch while retaining the screen that opened Settings.
        val root = entries.indexOfFirst { it == "settings" || it.startsWith("settings/") || it.startsWith("developer") || it == "experimental" }
        val prefix = if (root >= 0) entries.take(root) else entries
        val retainedParents = parents.mapIndexed { index, parent ->
            entries.getOrNull(prefix.size + index)?.takeIf { it.substringBefore('#') == parent } ?: parent
        }
        return RelayBackStack(prefix + retainedParents + route)
    }
    fun back() = if (canBack) RelayBackStack(entries.dropLast(1)) else this
    override fun equals(other: Any?) = other is RelayBackStack && entries == other.entries
    override fun hashCode() = entries.hashCode()
    override fun toString() = "RelayBackStack(entries=$entries)"
}

/** Anchors and guide steps select content within a page, not another page instance. */
fun relayPageKey(route: String) = if (route == "setup" || route.startsWith("setup:")) "setup" else route.substringBefore('#')

private fun relayNavigationRoute(entry: String) = relayRoute(entry).let { if (relayPageKey(it) == "home") "home" else it }

fun relaySetupStep(current: String, previousRequest: String?, request: String?) =
    if (request != null && request != previousRequest) request else current

fun relaySetupBack(origin: String, current: String, previous: String) = if (origin == current) previous else current

/** A launch arriving during a back gesture wins over the gesture's stale snapshot. */
fun commitRelayBack(origin: List<String>, current: List<String>): List<String> =
    if (origin == current) RelayBackStack(current).back().entries else RelayBackStack(current).entries
/** Old field links resolve to a destination plus an optional card/picker target. */
fun relayRoute(entry: String): String = when {
    entry.startsWith("settings:") -> when (entry.substringAfter(':')) {
        "output" -> "settings/output#output"
        "car" -> "settings/car#car"
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
        "experimental" -> "experimental"
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
    "developer", "experimental" -> listOf("settings")
    "developer/design" -> listOf("settings", "developer")
    else -> when {
        route.startsWith("developer/") -> listOf("settings", "developer")
        route.startsWith("settings/") -> listOf("settings")
        else -> emptyList()
    }
}

fun settingsSessionActive(entries: List<String>) = entries.any { it.startsWith("settings") || it.startsWith("developer") || it == "experimental" }

fun firstSettingsErrorPage(errors: Set<String>) = listOf("settings/output", "settings/connection/network",
    "settings/identity", "developer/video", "developer/connection", "experimental").firstOrNull { it in errors } ?: "settings"

fun showsDeveloperDiagnostics(route: String) = route in setOf("developer", "developer/diagnostics", "developer/carlink", "settings/identity")

fun showsIdentityStatus(route: String) = route == "settings"
