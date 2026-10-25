// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-25
package org.openxplay.direlay.ui

import androidx.compose.runtime.Composable
import androidx.compose.ui.res.stringResource
import org.openxplay.direlay.R
import org.openxplay.direlay.settings.CarSelection

internal fun homeCarRowVisible(car: CarSelection) = !car.complete

@Composable fun HomeCarRow(visible: Boolean, open: () -> Unit) {
    if (visible) SettingsRow(stringResource(R.string.car_choose_title), stringResource(R.string.car_choose_hint),
        R.drawable.symbol_car, action = open)
}
