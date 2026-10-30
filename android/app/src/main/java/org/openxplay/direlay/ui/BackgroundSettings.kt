// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-29
package org.openxplay.direlay.ui

import androidx.compose.runtime.*
import androidx.compose.ui.platform.LocalLifecycleOwner
import androidx.compose.ui.res.stringResource
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.LifecycleEventObserver
import org.openxplay.direlay.R
import org.openxplay.direlay.background.*
import org.openxplay.direlay.path.StepState
import org.openxplay.direlay.presentation.ScreenActions
import org.openxplay.direlay.presentation.ScreenSources
import org.openxplay.direlay.ui.design.*

@Composable internal fun backgroundState(sources: ScreenSources, actions: ScreenActions): State<BackgroundStatus?> {
    val lifecycle = LocalLifecycleOwner.current.lifecycle
    val refresh by rememberUpdatedState(actions.refreshBackground)
    DisposableEffect(lifecycle) {
        val observer = LifecycleEventObserver { _, event -> if (event == Lifecycle.Event.ON_RESUME) refresh() }
        lifecycle.addObserver(observer)
        onDispose { lifecycle.removeObserver(observer) }
    }
    return sources.background.screenState()
}

@Composable fun BackgroundSettings(status: BackgroundStatus?, open: (BackgroundDestination) -> Unit) {
    val view = status?.let(::presentBackground) ?: return
    InsetGroup(header = stringResource(R.string.background_system), footer = stringResource(R.string.background_footer)) {
        InsetRow(R.drawable.symbol_battery_charging, IconHue.PERMISSIONS, stringResource(R.string.background_battery_title),
            stringResource(if (view.batteryAllowed) R.string.background_allowed else R.string.background_battery_ask),
            onClick = { open(BackgroundDestination.BATTERY_OPTIMIZATION) }, trailing = {
                StatusGlyph(if (view.batteryAllowed) StepState.OK else StepState.ACTION)
            })
        if (view.showRestriction) {
            InsetDivider()
            InsetRow(R.drawable.symbol_battery_charging, IconHue.PERMISSIONS, stringResource(R.string.background_restricted_title),
                stringResource(R.string.background_restricted_hint), onClick = { open(BackgroundDestination.APP_DETAILS) })
        }
    }
    if (view.showVendorGroup) InsetGroup(header = stringResource(R.string.background_maker),
        footer = stringResource(R.string.background_recents_hint)) {
        view.vendorRows.forEachIndexed { index, kind ->
            if (index > 0) InsetDivider()
            val autostart = kind == VendorPageKind.AUTOSTART
            InsetRow(if (autostart) R.drawable.symbol_play else R.drawable.symbol_battery_charging, IconHue.PERMISSIONS,
                stringResource(if (autostart) R.string.background_autostart_title else R.string.background_vendor_battery_title),
                stringResource(if (autostart) R.string.background_autostart_hint else R.string.background_vendor_battery_hint),
                onClick = { open(if (autostart) BackgroundDestination.AUTOSTART else BackgroundDestination.BATTERY_SAVER) })
        }
    }
}
