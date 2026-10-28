// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-27
package org.openxplay.direlay

import android.app.AlertDialog

/** Activity-owned platform decisions wait for each other and for the screen to resume. */
internal class RelayDialogs(private val onVisible: (Boolean) -> Unit) {
    private val pending = ArrayDeque<AlertDialog>()
    private var current: AlertDialog? = null
    private var closed = false
    var active = false
        set(value) { field = value; showNext() }
    val busy get() = current != null || pending.isNotEmpty()

    fun show(dialog: AlertDialog, onDismiss: () -> Unit = {}) {
        if (closed || current === dialog || dialog in pending) return
        dialog.setOnDismissListener {
            if (current === dialog) current = null
            onDismiss()
            onVisible(busy)
            showNext()
        }
        pending.addLast(dialog)
        onVisible(true)
        showNext()
    }

    private fun showNext() {
        if (closed || !active || current != null || pending.isEmpty()) return
        current = pending.removeFirst().also { it.show() }
    }

    fun close() {
        closed = true
        pending.clear()
        current?.dismiss()
        current = null
        onVisible(false)
    }
}
