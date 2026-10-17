// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-14
package org.openxplay.direlay.bridge

import com.shilapi.xcertplay.airplay.MediaSink
import com.shilapi.xcertplay.airplay.SessionControl
import org.junit.jupiter.api.AfterEach
import org.openxplay.stream.core.ProjectionInput
import org.openxplay.stream.core.ProjectionInputContract

class DiPlayInputContractTest : ProjectionInputContract() {
    private lateinit var input: DiPlayInput
    override fun createInput(): ProjectionInput = DiPlayInput().also { input = it }
    override fun stimulate(input: ProjectionInput) {
        // A decorated engine is silent until the iPhone's main screen stream arrives.
        StreamBridgeSink(input as DiPlayInput, object : MediaSink {}, SessionControl { true }).onScreenStreamActive(110, true)
    }
    @AfterEach fun dispose() { if (::input.isInitialized) input.close() }
}
