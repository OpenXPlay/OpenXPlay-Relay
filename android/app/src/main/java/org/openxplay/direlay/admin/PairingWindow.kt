// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-16
package org.openxplay.direlay.admin

/** The parts of Android Bluetooth visibility the pairing window uses; Android offers no way to end it early. */
interface BluetoothVisibility {
    val available: Boolean
    /** Whether the system "make visible" dialog may be shown now (see [PairingVisibilityPolicy.dialogAllowed]). */
    fun dialogAllowed(): Boolean
    /**
     * Brings the pending prompt to the relay's screen: true when the relay app is in the foreground and
     * shows it now, false after posting a notification that opens it. Throws [PairingUnavailable] when it
     * can do neither.
     */
    fun prompt(): Boolean
    /** Removes the prompt's notification once the relay screen took the prompt or it expired. */
    fun withdraw()
    /** Whether the relay is discoverable now, or null when Android does not let us read it. */
    fun discoverable(): Boolean?
}
class PairingUnavailable(val key: String) : Exception(key)

/**
 * Tracks one pairing window, opened from the admin page or the app, for the status API; wall-clock seconds.
 * The relay screen takes the pending [Prompt], shows it and reports the user's answers back.
 */
class PairingWindow(private val visibility: BluetoothVisibility,
    private val clock: () -> Long = { System.currentTimeMillis() / 1000 }) {
    data class State(val deadline: Long? = null, val needsConfirmation: Boolean = false, val onRelay: Boolean = false,
        val needsPermission: Boolean = false, val permissionDenied: Boolean = false, val notified: Boolean = false)
    /** What the relay screen shows next. */
    enum class Prompt {
        /** Ask for the Nearby devices permission (Android 12+). */
        PERMISSION,
        /** The system "make visible" dialog. */
        DIALOG,
        /** System Bluetooth settings, which keep the relay visible while they stay open. */
        SETTINGS,
    }
    private enum class Step { PERMISSION, DIALOG, VISIBLE, SETTINGS }
    private var step: Step? = null
    private var since = 0L
    private var visibleSeconds = 0
    /** The prompt waits for the relay screen to take it. */
    private var waiting = false
    private var notified = false
    private var denied = false
    val available: Boolean get() = visibility.available

    @Synchronized fun open(): AdminResult {
        val now = clock()
        if (state(now) != State()) return AdminResult()
        if (!visibility.available) return AdminResult("error.pairing")
        begin(now, if (visibility.dialogAllowed()) Step.DIALOG else Step.PERMISSION)
        waiting = true
        return try {
            notified = !visibility.prompt()
            AdminResult()
        } catch (failure: PairingUnavailable) { close(); AdminResult(failure.key) }
    }

    /** The relay screen takes the pending prompt, once; null when nothing waits. */
    @Synchronized fun take(): Prompt? {
        val now = clock()
        state(now)
        if (!waiting) return null
        if (notified) visibility.withdraw()
        waiting = false; notified = false
        // Decide again: Nearby devices may have been allowed (or revoked) in Android settings meanwhile.
        begin(now, if (visibility.dialogAllowed()) Step.DIALOG else Step.PERMISSION)
        return if (step == Step.PERMISSION) Prompt.PERMISSION else Prompt.DIALOG
    }

    /** The answer to the Nearby devices request: the dialog follows, or Bluetooth settings when declined. */
    @Synchronized fun permissionResult(granted: Boolean): Prompt {
        begin(clock(), if (granted) Step.DIALOG else Step.SETTINGS)
        denied = !granted
        return if (granted) Prompt.DIALOG else Prompt.SETTINGS
    }

    /** The dialog's answer as visible seconds ([PairingVisibilityPolicy.visibleSeconds]); 0 when declined. */
    @Synchronized fun dialogResult(seconds: Int) {
        if (seconds <= 0) { close(); return }
        begin(clock(), Step.VISIBLE)
        visibleSeconds = seconds
    }

    /** The dialog could not be shown; Bluetooth settings keep the relay visible instead. */
    @Synchronized fun dialogFailed(permission: Boolean): Prompt {
        begin(clock(), Step.SETTINGS)
        denied = permission
        return Prompt.SETTINGS
    }

    /** Bluetooth settings could not be opened either. */
    @Synchronized fun settingsFailed() = close()

    @Synchronized fun state(now: Long = clock()): State {
        val current = step ?: return State()
        val limit = when {
            waiting && notified -> SECONDS // The user may first have to walk to the relay.
            current == Step.VISIBLE -> visibleSeconds
            current == Step.SETTINGS -> SECONDS
            else -> CONFIRM_SECONDS
        }
        val visible = if (current == Step.VISIBLE) visibility.discoverable() else null
        // Not visible after the grace period: the window ended early, for example with Bluetooth turned off.
        if (now >= since + limit || (visible == false && now - since >= CONFIRM_SECONDS)) {
            close()
            return State()
        }
        return when (current) {
            Step.PERMISSION -> State(needsPermission = true, notified = waiting && notified)
            Step.DIALOG -> State(needsConfirmation = true, notified = waiting && notified)
            Step.VISIBLE -> State(deadline = since + visibleSeconds)
            Step.SETTINGS -> State(onRelay = true, permissionDenied = denied)
        }
    }

    private fun begin(now: Long, next: Step) { step = next; since = now; denied = false }
    private fun close() {
        if (waiting && notified) visibility.withdraw()
        step = null; waiting = false; notified = false; denied = false
    }

    companion object {
        const val SECONDS = 600
        const val CONFIRM_SECONDS = 60
    }
}
