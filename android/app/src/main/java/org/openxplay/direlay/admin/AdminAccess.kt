// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-15
package org.openxplay.direlay.admin

import java.net.InetAddress

object AdminAccess {
    /** Test bytes directly: no DNS queries and no platform-dependent site-local classification. */
    fun isPrivate(address: InetAddress): Boolean = isPrivate(address.address)
    fun isPrivate(bytes: ByteArray): Boolean {
        val b = bytes.map { it.toInt() and 255 }
        return when (b.size) {
            4 -> b[0] == 10 || b[0] == 127 || (b[0] == 172 && b[1] in 16..31) ||
                (b[0] == 192 && b[1] == 168) || (b[0] == 169 && b[1] == 254)
            16 -> (b.take(15).all { it == 0 } && b[15] == 1) ||
                (b[0] == 254 && b[1] and 192 == 128) || (b[0] and 254 == 252) ||
                (b.take(10).all { it == 0 } && b[10] == 255 && b[11] == 255 && isPrivate(bytes.copyOfRange(12, 16)))
            else -> false
        }
    }
}
