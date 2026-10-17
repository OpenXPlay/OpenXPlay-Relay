// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-14
package org.openxplay.direlay.clink

/** Exactly one terminal response, including dismiss and repeated taps. */
class PinSheetState(private val confirmation: Boolean, private val respond: (String?) -> Unit) {
    var digits: String = ""
        private set
    var finished: Boolean = false
        private set
    val canSubmit: Boolean get() = !finished && (confirmation || digits.matches(Regex("[0-9]{6}")))
    fun edit(value: String) { if (!finished) digits = value.filter { it in '0'..'9' }.take(6) }
    fun submit() { if (canSubmit) finish(if (confirmation) "approved" else digits) }
    fun cancel() { if (!finished) finish(null) }
    private fun finish(value: String?) { finished = true; digits = ""; respond(value) }
}
