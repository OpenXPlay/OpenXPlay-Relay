// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-16
package org.openxplay.direlay.admin

import android.Manifest

/**
 * Decisions behind the pairing window's visibility, kept free of Android calls. The Nearby devices
 * permission is asked only when a pairing window opens, never at startup, so the relay runs without it.
 */
object PairingVisibilityPolicy {
    /** Android documents that the "make visible" dialog keeps the relay visible for at most 300 s per request. */
    const val DIALOG_MAX_SECONDS = 300
    private const val RESULT_OK = -1

    /**
     * What the dialog needs on Android 12+: BLUETOOTH_ADVERTISE for the request itself, and BLUETOOTH_CONNECT,
     * which guards the system activity that shows it. Both belong to Nearby devices, so Android asks once.
     */
    fun dialogPermissions(sdk: Int): List<String> = if (sdk < 31) emptyList()
        else listOf(Manifest.permission.BLUETOOTH_ADVERTISE, Manifest.permission.BLUETOOTH_CONNECT)

    fun missing(sdk: Int, granted: (String) -> Boolean): List<String> = dialogPermissions(sdk).filterNot(granted)

    fun dialogAllowed(sdk: Int, granted: (String) -> Boolean): Boolean = missing(sdk, granted).isEmpty()

    /** The visible time to ask the dialog for: the window, capped to what the system grants. */
    fun dialogSeconds(window: Int = PairingWindow.SECONDS): Int = window.coerceIn(1, DIALOG_MAX_SECONDS)

    /** The dialog answers with the visible seconds, or RESULT_CANCELED (0) when declined; some builds answer RESULT_OK. */
    fun visibleSeconds(resultCode: Int, requested: Int): Int = when {
        resultCode > 0 -> resultCode
        resultCode == RESULT_OK -> requested
        else -> 0
    }

    enum class Route {
        /** The relay app is in the foreground: its screen shows the prompt at once. */
        SCREEN,
        /** The relay app is in the background, where Android blocks dialogs: a notification opens it. */
        NOTIFICATION,
        /** In the background with notifications blocked: the request fails with a reason instead of silently. */
        NONE,
    }

    fun route(foreground: Boolean, notificationsEnabled: Boolean): Route = when {
        foreground -> Route.SCREEN
        notificationsEnabled -> Route.NOTIFICATION
        else -> Route.NONE
    }
}
