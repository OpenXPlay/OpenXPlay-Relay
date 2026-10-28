// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-27
package org.openxplay.direlay.relay

import org.junit.Assert.*
import org.junit.Test

class WirelessOutputLoggingTest {
    @Test fun statisticsLogImmediatelyThenAtMostEveryTenSecondsWithoutSlidingTheDeadline() {
        val limiter = WirelessStatisticsLogLimiter()
        assertTrue(limiter.allow(0))
        assertFalse(limiter.allow(0))
        assertFalse(limiter.allow(1_000_000_000))
        assertFalse(limiter.allow(9_999_999_999))
        assertTrue(limiter.allow(10_000_000_000))
        assertFalse(limiter.allow(19_999_999_999))
        assertTrue(limiter.allow(20_000_000_000))
        assertTrue(limiter.allow(60_000_000_000))
    }
    @Test fun outputsHaveIndependentLimitsAndMonotonicTimeCanStartNegativeOrWrap() {
        val dlna = WirelessStatisticsLogLimiter()
        val miracast = WirelessStatisticsLogLimiter()
        assertTrue(dlna.allow(-1))
        assertTrue(miracast.allow(-1))
        assertFalse(dlna.allow(0))
        assertTrue(dlna.allow(9_999_999_999))
        val wrapped = WirelessStatisticsLogLimiter()
        assertTrue(wrapped.allow(Long.MAX_VALUE - 1_000_000_000))
        assertFalse(wrapped.allow(Long.MIN_VALUE))
        assertTrue(wrapped.allow(Long.MIN_VALUE + 9_000_000_000))
    }
    @Test fun wirelessLinesRedactUrlsAndKeepMultilineFailuresOnOneLine() {
        val failure = "failure decoder_failed: token=test-secret\r\n" +
            "stream http://192.0.2.1:1234/stream/private-token.ts\n" +
            "source HTTPS://example.test/video?token=another-token\rtail"
        val line = wirelessLogLine(redactFailureText(failure))
        assertEquals("failure decoder_failed: token=[redacted] stream [redacted-url] source [redacted-url] tail", line)
        assertEquals("displays none", wirelessLogLine("displays none"))
    }
}
