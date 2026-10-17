// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-15
package org.openxplay.direlay.relay

import org.junit.Assert.*
import org.junit.Test

class WebPairingTest {
    @Test fun optionalPairingAcceptsEmptyStoredAndArbitraryTokens() {
        val verifier = webTokenVerifier(false, "01234567")
        for (token in listOf("", "01234567", "old code", "anything")) assertTrue(verifier.verify(token))
    }
    @Test fun requiredPairingAcceptsOnlyTheExactCodeIncludingLeadingZero() {
        val verifier = webTokenVerifier(true, "01234567")
        assertTrue(verifier.verify("01234567"))
        for (token in listOf("", "1234567", "01234568", "01234567 ", "０1234567")) assertFalse(verifier.verify(token))
    }
    @Test fun urlAndVisibleCodeFollowTheCurrentPolicy() {
        for (base in listOf("https://192.168.1.2:8443/", "https://192.168.1.2:8443/#k=old")) {
            assertEquals("https://192.168.1.2:8443/", receiverUrl(base, false, "01234567"))
            assertEquals("https://192.168.1.2:8443/#k=01234567", receiverUrl(base, true, "01234567"))
        }
        assertEquals("", visiblePairingCode(false, "01234567"))
        assertEquals("01234567", visiblePairingCode(true, "01234567"))
    }
}
