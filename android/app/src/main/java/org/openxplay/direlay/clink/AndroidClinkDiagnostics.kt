// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-15
package org.openxplay.direlay.clink

import android.util.Log

/** No Throwable is passed to Log: Android exception bodies may contain Wi-Fi credentials. */
class AndroidClinkDiagnostics : ClinkDiagnostics {
    override fun emit(event: ClinkDiagnostic) {
        if (event is ClinkDiagnostic.Failure) Log.w("DiRelayCarlink", event.line())
        else Log.i("DiRelayCarlink", event.line())
    }
}
