// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-25
package org.openxplay.direlay.update

import org.junit.Assert.*
import org.junit.Test

class ReleaseVersionTest {
    private fun version(value: String) = requireNotNull(ReleaseVersion.parse(value))

    @Test fun semverOrdersCoreNumbersAndPrereleaseIdentifiers() {
        val pairs = listOf("0.1.5" to "0.1.5-beta.2", "0.1.5-beta.2" to "0.1.5-beta.1",
            "0.1.6-beta.10" to "0.1.6-beta.2", "0.1.10" to "0.1.9", "0.1.5-rc.1" to "0.1.5-beta.9",
            "0.1.2" to "0.0.0-dev.abc1234", "1.0.0-alpha" to "1.0.0-999", "1.0.0-beta.1" to "1.0.0-beta",
            "999999999999999999999.0.0" to "999999999999999999998.0.0",
            "1.0.0-100000000000000000000" to "1.0.0-99999999999999999999")
        pairs.forEach { (higher, lower) ->
            assertTrue("$higher > $lower", version(higher) > version(lower))
            assertTrue("$lower < $higher", version(lower) < version(higher))
        }
        val ordered = listOf("1.0.0-alpha", "1.0.0-alpha.1", "1.0.0-alpha.beta", "1.0.0-beta",
            "1.0.0-beta.2", "1.0.0-beta.11", "1.0.0-rc.1", "1.0.0")
        assertEquals(ordered, ordered.reversed().map(::version).sorted().map { it.value })
    }
    @Test fun buildMetadataDoesNotChangePrecedence() {
        assertEquals(0, version("0.1.5+build.01").compareTo(version("0.1.5+other")))
        assertEquals(0, version("0.1.5-beta.2+abc").compareTo(version("0.1.5-beta.2")))
    }
    @Test fun invalidVersionsAreRejectedWithoutNumericOverflow() {
        listOf("", "v0.1.5", "android-v0.1.5", "0.1", "0.1.5.0", "01.1.5", "0.01.5", "0.1.05", "-1.0.0",
            "0.1.5-", "0.1.5-beta..1", "0.1.5-01", "0.1.5-beta.01", "0.1.5+", "0.1.5+a..b", "0.1.5+a+b",
            "0.1.5-beta_1", "0.1.5-{beta}", "0.1.5 ", " 0.1.5", "０.1.5").forEach { assertNull(it, ReleaseVersion.parse(it)) }
    }
}
