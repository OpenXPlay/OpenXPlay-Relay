// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-22
package org.openxplay.direlay.relay

import android.content.Context
import android.net.wifi.WifiManager
import java.net.Inet4Address
import org.openxplay.stream.cast.CastNetwork

class AndroidCastNetwork(context: Context, private val addresses: () -> List<Inet4Address>) : CastNetwork {
    private val wifi = context.applicationContext.getSystemService(WifiManager::class.java)
    override fun localAddresses(): List<Inet4Address> = addresses()
    override fun multicastLock(): AutoCloseable {
        val lock = wifi.createMulticastLock("openxplay-dlna").apply { setReferenceCounted(false); acquire() }
        return AutoCloseable { if (lock.isHeld) lock.release() }
    }
}
