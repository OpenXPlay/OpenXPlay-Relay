// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-22
package org.openxplay.direlay.relay

import java.lang.ref.WeakReference
import org.openxplay.stream.core.*

/** Track identities for as long as negotiation uses them, including during capability replacement. */
class CarDisplayMemory {
    private val declarations = mutableListOf<WeakReference<OutputCapabilities>>()
    @Synchronized private fun exclude(capabilities: OutputCapabilities) {
        declarations.removeAll { it.get() == null }
        if (declarations.none { it.get() === capabilities }) declarations.add(WeakReference(capabilities))
    }
    @Synchronized fun isWireless(capabilities: OutputCapabilities): Boolean = declarations.any { it.get() === capabilities }
    fun measuredDisplay(capabilities: OutputCapabilities?): DisplayInfo? =
        capabilities?.takeUnless(::isWireless)?.displays?.get(ScreenId.MAIN)

    fun exclude(output: ProjectionOutput): ProjectionOutput = object : ProjectionOutput by output {
        override fun open(host: OutputHost) = output.open(object : OutputHost by host {
            override fun capabilities(capabilities: OutputCapabilities) {
                exclude(capabilities)
                host.capabilities(capabilities)
            }
        })
    }
}
