// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-27
package org.openxplay.direlay.bridge

import com.shilapi.xcertplay.airplay.ScreenSessionPolicy
import org.openxplay.direlay.settings.ExperimentalOptions

/** Translate host choices at the input boundary, keeping settings out of the receiver component. */
internal fun ExperimentalOptions.toScreenSessionPolicy(): ScreenSessionPolicy = effective().let {
    ScreenSessionPolicy(it.screenSessionIsolation, it.screenReconnectReset, it.screenDeliverySerialization)
}
