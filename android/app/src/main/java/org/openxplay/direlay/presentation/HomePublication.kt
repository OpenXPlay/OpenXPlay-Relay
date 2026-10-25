// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-25
package org.openxplay.direlay.presentation

import org.openxplay.direlay.path.ConnectionPath
import org.openxplay.direlay.path.PathInput
import org.openxplay.direlay.ui.HomeContent

internal fun publishHome(input: PathInput, path: ConnectionPath, content: HomeContent): HomeContent =
    content.copy(pathInput = input.copy(nowMillis = 0), path = path)
