// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-22
package org.openxplay.direlay.miracast

import android.app.Application
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [33], application = Application::class)
class MiracastFrameQueueTest {
    @Test fun aNewQueueAndAClearedQueueGateOnAKeyframe() {
        val queue = MiracastFrameQueue()
        queue.offer(frame())
        assertEquals(0, queue.size)
        queue.offer(frame(true))
        queue.offer(frame())
        assertEquals(2, queue.size)
        queue.clear()
        queue.offer(frame())
        assertEquals(0, queue.size)
        assertEquals(4L, queue.dropped)
    }
    @Test fun overflowRetainsTheNextQueuedKeyframeAndItsDependents() {
        val queue = MiracastFrameQueue()
        repeat(8) { assertFalse(queue.offer(frame(it == 0 || it == 5, it.toLong()))) }
        assertTrue(queue.offer(frame(pts = 8)))
        assertEquals(4, queue.size)
        assertEquals(5L, queue.dropped)
        assertTrue(queue.peek()!!.keyFrame)
        assertEquals(listOf(5L, 6L, 7L, 8L), List(queue.size) { queue.remove()!!.ptsUs })
    }
    @Test fun overflowWithoutAnotherKeyframeDropsUntilOneArrives() {
        val queue = MiracastFrameQueue()
        repeat(8) { queue.offer(frame(it == 0, it.toLong())) }
        assertTrue(queue.offer(frame(pts = 8)))
        assertEquals(0, queue.size)
        assertEquals(9L, queue.dropped)
        queue.offer(frame())
        val key = frame(true, 10)
        queue.offer(key)
        assertSame(key, queue.peek())
        repeat(100) { queue.offer(frame(it % 10 == 0, 11L + it)); assertTrue(queue.size <= 8) }
    }
}
