// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-11-02
package org.openxplay.direlay.carlife

import java.io.Closeable
import java.io.InputStream
import org.junit.Assert.*
import org.junit.Test

class AccessoryAttachmentTest {
    @Test fun shortFramingReadsPreserveCompleteUsbTransfers() {
        val packets = ArrayDeque(listOf(ByteArray(8) { it.toByte() }, ByteArray(11) { (it + 8).toByte() }))
        val underlying = object : InputStream() {
            override fun read(): Int = error("USB must read a whole transfer")
            override fun read(bytes: ByteArray, offset: Int, length: Int): Int {
                if (packets.isEmpty()) return -1
                val transfer = packets.removeFirst()
                val read = minOf(length, transfer.size)
                transfer.copyInto(bytes, offset, 0, read)
                // The remainder of this transfer is deliberately dropped, like Android AOA.
                return read
            }
        }
        val input = bufferAccessoryInput(underlying)
        val outer = ByteArray(8)
        val inner = ByteArray(8)
        val payload = ByteArray(3)
        assertEquals(8, input.read(outer))
        assertEquals(8, input.read(inner))
        assertEquals(3, input.read(payload))
        assertArrayEquals(ByteArray(19) { it.toByte() }, outer + inner + payload)
        assertEquals(-1, input.read())
    }

    @Test fun onlyCarlifeDescriptorsMatch() {
        assertTrue(carlifeAccessoryMatches("Baidu", "CarLife"))
        assertFalse(carlifeAccessoryMatches("Baidu", "Android Auto"))
        assertFalse(carlifeAccessoryMatches("Other", "CarLife"))
        assertFalse(carlifeAccessoryMatches(null, "CarLife"))
        assertFalse(carlifeAccessoryMatches("Baidu", null))
    }

    @Test fun detachedDeviceCannotInstallLateOpen() {
        val lifecycle = AccessoryAttachment<String>()
        val opening = lifecycle.generation()
        var closed = 0
        lifecycle.clear()
        assertFalse(lifecycle.install("old", opening, Closeable { closed++ }))
        assertEquals(1, closed)
        assertFalse(lifecycle.contains("old"))
        val fresh = lifecycle.generation()
        assertTrue(lifecycle.install("new", fresh, Closeable { closed++ }))
        assertTrue(lifecycle.contains("new"))
        lifecycle.close()
        assertEquals(2, closed)
    }

    @Test fun stoppingWhilePlatformOpensClosesLateLeaseExactlyOnce() {
        val lifecycle = AccessoryAttachment<String>()
        val opening = lifecycle.generation()
        var closed = 0
        lifecycle.close()
        assertFalse(lifecycle.isCurrent(opening))
        assertFalse(lifecycle.install("late", opening, Closeable { closed++ }))
        lifecycle.close()
        assertEquals(1, closed)
    }

    @Test fun attachmentReplacementClosesOnlyPreviousLease() {
        val lifecycle = AccessoryAttachment<String>()
        var first = 0
        var second = 0
        assertTrue(lifecycle.install("first", lifecycle.generation(), Closeable { first++ }))
        assertFalse(lifecycle.install("duplicate", lifecycle.generation(), Closeable { second++ }))
        assertEquals(0, first)
        assertEquals(1, second)
        assertTrue(lifecycle.clear())
        assertFalse(lifecycle.clear())
        assertEquals(1, first)
        assertTrue(lifecycle.install("replacement", lifecycle.generation(), Closeable { second++ }))
        lifecycle.close()
        lifecycle.close()
        assertEquals(2, second)
    }
}
