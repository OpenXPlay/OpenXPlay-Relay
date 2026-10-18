// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-18
package org.openxplay.direlay.bridge

/**
 * DiPlay hands over everything after the hvcC fourcc of the iPhone's sample description, so boxes
 * that follow hvcC there (such as colr) trail the record. Returns just the hvcC record, measured
 * from its own parameter-set arrays, or a copy of [data] when it does not have that shape.
 */
internal fun hevcRecord(data: ByteArray): ByteArray {
    // The fixed hvcC fields take 22 bytes; numOfArrays follows.
    if (data.size < 23) return data.copyOf()
    fun u16(at: Int) = ((data[at].toInt() and 0xff) shl 8) or (data[at + 1].toInt() and 0xff)
    var offset = 23
    repeat(data[22].toInt() and 0xff) {
        if (offset + 3 > data.size) return data.copyOf()
        val count = u16(offset + 1)
        offset += 3
        repeat(count) {
            if (offset + 2 > data.size) return data.copyOf()
            offset += 2 + u16(offset)
            if (offset > data.size) return data.copyOf()
        }
    }
    return data.copyOfRange(0, offset)
}
