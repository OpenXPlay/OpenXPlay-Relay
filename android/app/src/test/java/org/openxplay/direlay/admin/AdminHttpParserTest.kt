// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-15
package org.openxplay.direlay.admin

import java.io.ByteArrayInputStream
import org.junit.Assert.*
import org.junit.Test

class AdminHttpParserTest {
    private fun parse(text: String) = AdminServer.readRequest(ByteArrayInputStream(text.toByteArray()))
    private fun rejected(status: Int, text: String) {
        try { parse(text); fail("Expected HTTP $status") }
        catch (failure: AdminServer.HttpFailure) { assertEquals(status, failure.status) }
    }
    @Test fun bodyBoundaryAndHeaders() {
        val body = "a".repeat(65536)
        assertEquals(65536, parse("PUT /api/v0/settings HTTP/1.1\r\nHost: local\r\nContent-Length: 65536\r\n\r\n$body").body.size)
        rejected(413, "POST / HTTP/1.1\r\nHost: local\r\nContent-Length: 65537\r\n\r\n")
        rejected(431, "GET / HTTP/1.1\r\nHost: local\r\nX-Large: ${"a".repeat(16384)}\r\n\r\n")
    }
    @Test fun ambiguousFramingAndTruncationAreRejected() {
        rejected(400, "POST / HTTP/1.1\r\nHost: local\r\nContent-Length: 1\r\nContent-Length: 1\r\n\r\na")
        rejected(400, "POST / HTTP/1.1\r\nHost: local\r\nTransfer-Encoding: chunked\r\n\r\n")
        rejected(400, "POST / HTTP/1.1\r\nHost: local\r\nContent-Length: -1\r\n\r\n")
        rejected(400, "POST / HTTP/1.1\r\nHost: local\r\nContent-Length: 2\r\n\r\na")
        rejected(400, "GET / HTTP/1.1\r\n\r\n")
    }
}
