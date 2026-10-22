// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-22
package org.openxplay.direlay.relay

import com.shilapi.xcertplay.iap2.wire.Iap2Frame
import com.shilapi.xcertplay.orchestration.CarPlayEventObserver
import com.shilapi.xcertplay.orchestration.CarPlayStatus

/** Optional plugins observe the same events independently of diagnostics. */
internal class CompositeCarPlayObserver(private val diagnostics: CarPlayEventObserver,
    private val plugins: CarPlayEventObserver? = null) : CarPlayEventObserver {
    private inline fun forward(call: CarPlayEventObserver.() -> Unit) {
        runCatching { diagnostics.call() }
        runCatching { plugins?.call() }
    }
    override fun onStatus(status: CarPlayStatus) = forward { onStatus(status) }
    override fun onProgress(message: String) = forward { onProgress(message) }
    override fun onSessionActive() = forward { onSessionActive() }
    override fun onSessionEnded() = forward { onSessionEnded() }
    override fun onIap2Frame(frame: Iap2Frame) = forward { onIap2Frame(frame) }
    override fun onCommand(type: String, params: Map<String, Any?>) = forward { onCommand(type, params) }
}
