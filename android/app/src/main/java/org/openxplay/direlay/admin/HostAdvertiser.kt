// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-15
package org.openxplay.direlay.admin

import android.content.Context
import android.net.wifi.WifiManager
import java.net.InetAddress
import javax.jmdns.JmDNS
import javax.jmdns.ServiceInfo

interface HostAdvertiser : AutoCloseable {
    val host: String?
    fun advertise(ipv4: String, receiverPort: Int, version: String)
}
/** Called only on the admin lifecycle worker, using RelayRuntime's car-facing address set. */
class JmDnsHostAdvertiser(context: Context) : HostAdvertiser {
    private val wifi = context.applicationContext.getSystemService(WifiManager::class.java)
    private var lock: WifiManager.MulticastLock? = null
    private var dns: JmDNS? = null
    private var config: Triple<String, Int, String>? = null
    override val host: String? get() = dns?.hostName?.trimEnd('.')
    override fun advertise(ipv4: String, receiverPort: Int, version: String) {
        val requested = Triple(ipv4, receiverPort, version)
        if (requested == config) return
        close()
        try {
            val multicast = wifi.createMulticastLock("direlay-admin-mdns").apply { setReferenceCounted(false); acquire() }
            lock = multicast
            val instance = JmDNS.create(InetAddress.getByName(ipv4), "openxplay-android")
            dns = instance
            instance.registerService(ServiceInfo.create("_openxplay._tcp.local.", "OpenXPlay Relay", AdminServer.PORT,
                0, 0, mapOf("platform" to "android", "receiver" to receiverPort.toString(), "version" to version)))
            config = requested
        } catch (failure: Exception) { close(); throw failure }
    }
    override fun close() {
        try { dns?.close() } finally {
            dns = null; config = null
            lock?.let { if (it.isHeld) it.release() }; lock = null
        }
    }
}
