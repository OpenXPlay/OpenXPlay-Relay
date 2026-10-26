// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-23
package org.openxplay.direlay.clink

internal data class BleScanPolicy(val timeoutSeconds: Long?, val restartMs: Long?) {
    companion object {
        fun forAttempt(keepScanning: Boolean) =
            if (keepScanning) BleScanPolicy(null, 10 * 60 * 1000L) else BleScanPolicy(8, null)
    }
}
