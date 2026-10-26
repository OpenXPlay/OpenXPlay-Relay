// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-25
package org.openxplay.direlay.ui.design

import androidx.compose.animation.*
import androidx.compose.ui.Alignment

internal fun relayDisclosureEnter(motion: Boolean): EnterTransition =
    expandVertically(relaySpatial(motion), expandFrom = Alignment.Top) + fadeIn(relayEffects(motion))

internal fun relayDisclosureExit(motion: Boolean): ExitTransition =
    shrinkVertically(relaySpatial(motion), shrinkTowards = Alignment.Top) + fadeOut(relayEffects(motion))
