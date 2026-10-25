// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-25
package org.openxplay.direlay.ui

import androidx.compose.runtime.*
import androidx.compose.ui.platform.LocalLifecycleOwner
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.repeatOnLifecycle
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.collect

@Composable internal fun <T> StateFlow<T>.screenState(): State<T> {
    val lifecycle = LocalLifecycleOwner.current.lifecycle
    return produceState(value, this, lifecycle) {
        lifecycle.repeatOnLifecycle(Lifecycle.State.STARTED) { this@screenState.collect { value = it } }
    }
}
