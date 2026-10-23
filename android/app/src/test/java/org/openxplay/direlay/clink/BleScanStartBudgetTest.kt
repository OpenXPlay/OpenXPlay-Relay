// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-23
package org.openxplay.direlay.clink

import org.junit.Assert.*
import org.junit.Test

class BleScanStartBudgetTest {
    @Test fun allowsAtMostFiveStartsWithoutReserve() {
        val budget = BleScanStartBudget()
        repeat(5) { assertTrue(budget.tryStart(0, reserve = 0)) }
        assertFalse(budget.tryStart(0, reserve = 0))
        assertEquals(5, budget.count(0))
    }

    @Test fun reservesTwoStartsAndDoesNotCountRejectedStarts() {
        val budget = BleScanStartBudget()
        repeat(3) { assertTrue(budget.tryStart(0, reserve = 2)) }
        repeat(3) { assertFalse(budget.tryStart(1, reserve = 2)) }
        assertEquals(3, budget.count(1))
        repeat(2) { assertTrue(budget.tryStart(1, reserve = 0)) }
        assertFalse(budget.tryStart(1, reserve = 0))
    }

    @Test fun startsExpireAtThirtySeconds() {
        val budget = BleScanStartBudget()
        repeat(3) { assertTrue(budget.tryStart(100, reserve = 2)) }
        assertFalse(budget.tryStart(30_099, reserve = 2))
        assertEquals(3, budget.count(30_099))
        assertTrue(budget.tryStart(30_100, reserve = 2))
        assertEquals(1, budget.count(30_100))
    }

    @Test fun windowExpiresEachStartIndividually() {
        val budget = BleScanStartBudget()
        listOf(0L, 10_000L, 20_000L).forEach { assertTrue(budget.tryStart(it, reserve = 2)) }
        assertTrue(budget.tryStart(30_000, reserve = 2))
        assertFalse(budget.tryStart(39_999, reserve = 2))
        assertTrue(budget.tryStart(40_000, reserve = 2))
        assertEquals(3, budget.count(40_000))
    }

    @Test fun filteredStartsConsumeRawBudgetAndKeepTheirReservedHeadroom() {
        val budget = BleScanStartBudget()
        budget.recordStart(0)
        assertTrue(budget.tryStart(0, reserve = 2))
        budget.recordStart(1_000)
        assertFalse(budget.tryStart(1_000, reserve = 2))
        budget.recordStart(2_000)
        budget.recordStart(3_000)
        assertEquals(5, budget.count(3_000))
        assertFalse(budget.tryStart(3_000, reserve = 2))
    }

    @Test fun filteredStartsAreRecordedEvenWhenBudgetIsFull() {
        val budget = BleScanStartBudget()
        repeat(6) { budget.recordStart(0) }
        assertEquals(6, budget.count(0))
        assertFalse(budget.tryStart(0, reserve = 0))
        assertFalse(budget.tryStart(0, reserve = 2))
        assertEquals(0, budget.count(30_000))
        assertTrue(budget.tryStart(30_000, reserve = 2))
    }
}
