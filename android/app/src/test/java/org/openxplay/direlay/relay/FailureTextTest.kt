// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-21
package org.openxplay.direlay.relay

import java.io.IOException
import org.junit.Assert.*
import org.junit.Test

class FailureTextTest {
    @Test fun keepsRealMessagesAndCauseChainIncludingProtocolCodes() {
        val failure = IllegalStateException("Carlink failed", IOException("auth_rejected", IOException("Socket closed")))
        assertEquals("Carlink failed\nCaused by: auth_rejected\nCaused by: Socket closed", failureText(failure))
    }

    @Test fun usesExceptionClassForNullEmptyOrBlankMessagesAtEveryLevel() {
        for (message in listOf(null, "", "  ")) {
            assertEquals("IllegalStateException\nCaused by: IOException",
                failureText(IllegalStateException(message, IOException(message))))
        }
    }

    @Test fun limitsCauseChainToFiveLevelsAndHandlesCycles() {
        var failure: Throwable = IOException("level 6")
        for (level in 5 downTo 0) failure = IllegalStateException("level $level", failure)
        assertEquals("level 0\nCaused by: level 1\nCaused by: level 2\nCaused by: level 3\nCaused by: level 4",
            failureText(failure))
        val outer = IllegalStateException("outer")
        val inner = IOException("inner", outer)
        outer.initCause(inner)
        assertEquals("outer\nCaused by: inner", failureText(outer))
    }

    @Test fun redactsCredentialFieldsAndUrlCredentialsWithoutHidingProtocolCodesOrPorts() {
        val text = failureText(IllegalStateException(
            "remoteToken=remote-secret password='two words' PIN: 1234 Bearer bearer-secret " +
                "https://user:pass@example.test:9443/?token=query-secret auth_pin_proof_failed"))
        for (secret in listOf("remote-secret", "two words", "1234", "bearer-secret", "user:pass", "query-secret"))
            assertFalse(text, text.contains(secret))
        assertTrue(text.contains("[redacted]"))
        assertTrue(text.contains("auth_pin_proof_failed"))
        assertTrue(text.contains(":9443/"))
    }

    @Test fun redactsKnownUnlabelledSecretsThroughoutTheCauseChain() {
        val failure = IllegalStateException("Remote failed with fake-token", IOException("Rejected fake-wifi and 765432"))
        assertEquals("Remote failed with [redacted]\nCaused by: Rejected [redacted] and [redacted]",
            failureText(failure) { redactFailureText(it, listOf("", "fake-token", "fake-wifi", "765432")) })
    }
}
