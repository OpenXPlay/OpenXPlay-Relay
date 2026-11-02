// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-22
package org.openxplay.direlay.relay

import org.openxplay.direlay.miracast.MiracastPicture
import org.openxplay.direlay.settings.WirelessPictureMode
import org.openxplay.direlay.settings.WirelessPictureSize
import org.openxplay.stream.core.Viewport
import org.openxplay.stream.core.OutputHost
import org.openxplay.stream.core.OutputCapabilities
import org.openxplay.stream.core.ProjectionOutput
import org.openxplay.stream.core.ScreenId
import org.openxplay.direlay.settings.RelaySettings

data class DlnaOutputPlan(val udn: String, val size: WirelessPictureSize, val densityDpi: Int = 160)
data class WirelessDisplayFailure(val code: String, val detail: String, val atMillis: Long)

object WirelessOutputPolicy {
    fun dlna(settings: RelaySettings, picture: WirelessPictureResolution = WirelessPicturePolicy.resolve(settings)): DlnaOutputPlan? =
        settings.dlnaRenderer?.takeIf { settings.dlnaOutput }?.let { DlnaOutputPlan(it.udn, picture.size, picture.densityDpi) }
    // CastOutputConfig has no density field; preserve the screen declaration at the relay boundary.
    fun declareDlna(output: ProjectionOutput, plan: DlnaOutputPlan): ProjectionOutput = object : ProjectionOutput by output {
        override fun open(host: OutputHost) = output.open(object : OutputHost by host {
            override fun capabilities(capabilities: OutputCapabilities) {
                val display = capabilities.displays[ScreenId.MAIN]
                host.capabilities(if (display == null || display.densityDpi == plan.densityDpi) capabilities else capabilities.copy(
                    displays = capabilities.displays + (ScreenId.MAIN to display.copy(densityDpi = plan.densityDpi))))
            }
        })
    }
    fun miracast(settings: RelaySettings): MiracastPicture? = if (!settings.miracastOutput) null else MiracastPicture(
        size = settings.wirelessPictureSize.fitted().takeIf { settings.wirelessPicture == WirelessPictureMode.CUSTOM }?.let { Viewport(it.width, it.height) },
        stretch = settings.miracastStretch)
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
