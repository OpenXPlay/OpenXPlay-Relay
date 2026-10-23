// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-22
package org.openxplay.direlay.relay

import org.openxplay.direlay.settings.DlnaPicture
import org.openxplay.direlay.settings.RelaySettings

data class DlnaOutputPlan(val udn: String, val picture: DlnaPicture)
data class WirelessDisplayFailure(val code: String, val detail: String, val atMillis: Long)

object WirelessOutputPolicy {
    fun dlna(settings: RelaySettings): DlnaOutputPlan? =
        settings.dlnaRenderer?.takeIf { settings.dlnaOutput }?.let { DlnaOutputPlan(it.udn, settings.dlnaPicture) }
    fun miracast(settings: RelaySettings): Unit? = Unit.takeIf { settings.miracastOutput }
}

enum class OutputChange { KEEP, REMOVE, ADD }

/** Confined to setup. A removed output must finish closing before its id can be reused. */
class RetiringOutputPolicy<T : Any> {
    var current: T? = null
        private set
    var retiring = false
        private set
    fun update(requested: T?): OutputChange = when {
        retiring || current == requested -> OutputChange.KEEP
        current != null -> { retiring = true; OutputChange.REMOVE }
        else -> { current = requested; OutputChange.ADD }
    }
    fun ended() { current = null; retiring = false }
}
