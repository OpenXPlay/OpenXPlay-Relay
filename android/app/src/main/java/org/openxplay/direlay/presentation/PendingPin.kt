// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-25
package org.openxplay.direlay.presentation

data class PendingPin(val headUnitId: String, val confirmation: Boolean, val incorrect: Boolean,
    val respond: (String?) -> Unit, val cancel: () -> Unit)
