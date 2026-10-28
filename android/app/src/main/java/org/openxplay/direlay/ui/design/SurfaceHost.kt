// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-27
package org.openxplay.direlay.ui.design

import androidx.compose.material3.AlertDialog
import androidx.compose.material3.AlertDialogDefaults
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Shape
import androidx.compose.ui.window.DialogProperties

internal data class RelaySurfaceRequest(val owner: Any, val priority: Int)

/** Visibility is serialized; the requesting screen still owns its decision or sheet state. */
internal data class RelaySurfaceQueue(val requests: List<RelaySurfaceRequest> = emptyList()) {
    val visible get() = requests.maxByOrNull { it.priority }?.owner
    fun open(owner: Any, priority: Int) = if (RelaySurfaceRequest(owner, priority) in requests) this else
        copy(requests = requests.filterNot { it.owner == owner } + RelaySurfaceRequest(owner, priority))
    fun close(owner: Any) = copy(requests = requests.filterNot { it.owner == owner })
}

/** Informational and progress content stays a dismissible surface over its owning page. */
@Composable fun RelayInfoSheet(onDismissRequest: () -> Unit, confirmButton: @Composable () -> Unit,
    dismissButton: (@Composable () -> Unit)? = null, title: (@Composable () -> Unit)? = null,
    text: (@Composable () -> Unit)? = null) {
    RelaySheet(onDismissRequest) {
        title?.invoke()
        text?.invoke()
        Column(Modifier.fillMaxWidth(), horizontalAlignment = Alignment.End) {
            confirmButton()
            dismissButton?.invoke()
        }
    }
}

internal val LocalRelaySurfaceHost = staticCompositionLocalOf<MutableState<RelaySurfaceQueue>?> { null }
internal val LocalRelaySurfaceActive = compositionLocalOf { true }
internal val LocalRelaySurfacePriority = compositionLocalOf { 0 }

@Composable internal fun RelaySurface(content: @Composable () -> Unit) {
    if (!LocalRelaySurfaceActive.current) return
    val host = LocalRelaySurfaceHost.current
    if (host == null) { content(); return }
    val owner = remember { Any() }
    val priority = LocalRelaySurfacePriority.current
    DisposableEffect(host, owner, priority) {
        host.value = host.value.open(owner, priority)
        onDispose { host.value = host.value.close(owner) }
    }
    if (host.value.visible == owner) content()
}

@Composable fun RelayDecisionDialog(onDismissRequest: () -> Unit, confirmButton: @Composable () -> Unit,
    dismissButton: (@Composable () -> Unit)? = null, title: (@Composable () -> Unit)? = null,
    text: (@Composable () -> Unit)? = null, icon: (@Composable () -> Unit)? = null,
    shape: Shape = AlertDialogDefaults.shape, containerColor: Color = AlertDialogDefaults.containerColor,
    properties: DialogProperties = DialogProperties()) {
    RelaySurface {
        AlertDialog(onDismissRequest = onDismissRequest, confirmButton = confirmButton,
            dismissButton = dismissButton, title = title, text = text, icon = icon, shape = shape,
            containerColor = containerColor, properties = properties)
    }
}
