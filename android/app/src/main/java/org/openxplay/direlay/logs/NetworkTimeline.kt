// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-23
package org.openxplay.direlay.logs

import android.bluetooth.BluetoothManager
import android.bluetooth.BluetoothAdapter
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.net.ConnectivityManager
import android.net.Network
import android.net.NetworkCapabilities
import android.net.NetworkRequest
import android.net.wifi.WifiInfo
import android.net.wifi.WifiManager
import android.net.wifi.p2p.WifiP2pManager
import android.os.Build
import android.os.Handler
import android.os.HandlerThread
import org.openxplay.direlay.network.AndroidNetworkStateReader

/** One main-process observer. Platform queries run on its handler, never on media callbacks. */
class NetworkTimeline(context: Context, private val log: ConnectionLog) : AutoCloseable {
    private val context = context.applicationContext
    private val thread = HandlerThread("relay-network-timeline").apply { start() }
    private val handler = Handler(thread.looper)
    private val wifi = context.getSystemService(WifiManager::class.java)
    private val connectivity = context.getSystemService(ConnectivityManager::class.java)
    private var registered = false
    private var callbackRegistered = false
    private var p2pChannel: WifiP2pManager.Channel? = null
    private val started = java.util.concurrent.atomic.AtomicBoolean()
    private val closed = java.util.concurrent.atomic.AtomicBoolean()
    private val networks = linkedMapOf<Network, String>()
    private val callback = object : ConnectivityManager.NetworkCallback() {
        override fun onCapabilitiesChanged(network: Network, capabilities: NetworkCapabilities) = safely {
            if (!capabilities.hasTransport(NetworkCapabilities.TRANSPORT_WIFI)) return@safely
            val info = if (Build.VERSION.SDK_INT >= 29) capabilities.transportInfo as? WifiInfo else null
            remember(network, wifiDetail(info ?: currentWifi()))
            connected()
        }
        override fun onLost(network: Network) = safely { networks.remove(network); connected() }
    }
    private val receiver = object : BroadcastReceiver() {
        override fun onReceive(context: Context, intent: Intent) = safely {
            when (intent.action) {
                WifiManager.WIFI_STATE_CHANGED_ACTION -> wifiState(intent.getIntExtra(WifiManager.EXTRA_WIFI_STATE, WifiManager.WIFI_STATE_UNKNOWN))
                WifiManager.NETWORK_STATE_CHANGED_ACTION -> wifiConnection()
                TETHER -> hotspot(intent)
                BluetoothAdapter.ACTION_STATE_CHANGED -> bluetooth(intent.getIntExtra(BluetoothAdapter.EXTRA_STATE, BluetoothAdapter.ERROR))
                WifiP2pManager.WIFI_P2P_CONNECTION_CHANGED_ACTION -> p2p()
            }
        }
    }
    fun start() {
        if (!started.compareAndSet(false, true) || closed.get()) return
        handler.post { safely {
            if (closed.get()) return@safely
            val filter = IntentFilter().apply {
                addAction(WifiManager.WIFI_STATE_CHANGED_ACTION); addAction(WifiManager.NETWORK_STATE_CHANGED_ACTION)
                addAction(TETHER); addAction(BluetoothAdapter.ACTION_STATE_CHANGED)
                addAction(WifiP2pManager.WIFI_P2P_CONNECTION_CHANGED_ACTION)
            }
            runCatching {
                if (Build.VERSION.SDK_INT >= 33) context.registerReceiver(receiver, filter, null, handler, Context.RECEIVER_EXPORTED)
                else @Suppress("DEPRECATION") context.registerReceiver(receiver, filter, null, handler)
                registered = true
            }
                .onFailure { unavailable("broadcasts") }
            runCatching {
                connectivity.registerNetworkCallback(NetworkRequest.Builder().addTransportType(NetworkCapabilities.TRANSPORT_WIFI)
                    .removeCapability(NetworkCapabilities.NET_CAPABILITY_INTERNET).build(), callback, handler)
                callbackRegistered = true
            }.onFailure { unavailable("wifi_connection") }
            runCatching { wifiState(wifi.wifiState) }.onFailure { unavailable("wifi") }
            wifiConnection(); hotspot(); p2p()
            runCatching { bluetooth(context.getSystemService(BluetoothManager::class.java)?.adapter?.state ?: BluetoothAdapter.ERROR) }
                .onFailure { unavailable("bluetooth") }
        } }
    }
    private fun wifiState(state: Int) {
        when (state) {
            WifiManager.WIFI_STATE_ENABLED -> state("wifi", "wifi_enabled")
            WifiManager.WIFI_STATE_DISABLED -> state("wifi", "wifi_disabled")
            WifiManager.WIFI_STATE_UNKNOWN -> unavailable("wifi")
        }
    }
    private fun bluetooth(value: Int) {
        when (value) {
            BluetoothAdapter.STATE_ON -> state("bluetooth", "bluetooth_on")
            BluetoothAdapter.STATE_OFF -> state("bluetooth", "bluetooth_off")
            BluetoothAdapter.ERROR -> unavailable("bluetooth")
        }
    }
    @Suppress("DEPRECATION") private fun currentWifi(): WifiInfo? = runCatching { wifi.connectionInfo }.getOrNull()
    private fun wifiDetail(info: WifiInfo?): String {
        val frequency = info?.frequency ?: -1
        if (info == null || info.ssid == WifiManager.UNKNOWN_SSID || frequency <= 0) unavailable("wifi_identity")
        val band = when (frequency) { in 2400..2500 -> "2.4GHz"; in 4900..5900 -> "5GHz"; in 5925..7125 -> "6GHz"; else -> "unavailable" }
        return "ssid=${info?.ssid ?: "unavailable"} frequency=$frequency band=$band"
    }
    @Suppress("DEPRECATION") private fun wifiConnection() {
        runCatching {
            val active = connectivity.allNetworks.filter { connectivity.getNetworkCapabilities(it)
                ?.hasTransport(NetworkCapabilities.TRANSPORT_WIFI) == true }
            networks.keys.retainAll(active.toSet())
            for (network in active) remember(network, wifiDetail(currentWifi()))
            connected()
        }.onFailure { unavailable("wifi_connection") }
    }
    /** Readings without location access report <unknown ssid>; keep the identity already seen for that network. */
    private fun remember(network: Network, detail: String) {
        val previous = networks[network]
        networks[network] = if (previous != null && detail.startsWith("ssid=${WifiManager.UNKNOWN_SSID} ")) previous else detail
    }
    private fun connected() {
        val detail = networks.values.sorted().joinToString("; ")
        if (networks.isEmpty()) state("wifi_connection", "wifi_disconnected")
        else state("wifi_connection", "wifi_connected", detail)
    }
    private fun hotspot(intent: Intent? = null) {
        val key = listOf("tetherArray", "activeArray").firstOrNull { intent?.hasExtra(it) == true }
        val enabled = if (key == null) AndroidNetworkStateReader(context).hotspotEnabled()
            else org.openxplay.direlay.network.wifiTetheringEnabled(intent?.getStringArrayListExtra(key))
        when (enabled) {
            true -> state("hotspot", "hotspot_on")
            false -> state("hotspot", "hotspot_off")
            null -> unavailable("hotspot")
        }
    }
    private fun p2p() {
        runCatching {
            val manager = context.getSystemService(WifiP2pManager::class.java) ?: return unavailable("wifi_direct")
            val channel = p2pChannel ?: manager.initialize(context, thread.looper, null)?.also { p2pChannel = it }
                ?: return unavailable("wifi_direct")
            manager.requestGroupInfo(channel) { group -> safely {
                    if (group == null) state("wifi_direct", "p2p_removed")
                    else {
                        val frequency = if (Build.VERSION.SDK_INT >= 29) group.frequency else -1
                        if (frequency <= 0) unavailable("wifi_direct_frequency")
                        state("wifi_direct", "p2p_formed", "role=${if (group.isGroupOwner) "owner" else "client"} frequency=$frequency")
                    }
                } }
        }.onFailure { unavailable("wifi_direct") }
    }
    private fun state(key: String, event: String, detail: String = "") = log.transition("network", key, event, detail)
    private fun unavailable(key: String) = log.transition("network", "$key/unavailable", "unavailable", "state=$key")
    private inline fun safely(action: () -> Unit) { runCatching(action).onFailure { unavailable("observer") } }
    override fun close() {
        if (!closed.compareAndSet(false, true)) return
        handler.post {
            if (registered) runCatching { context.unregisterReceiver(receiver) }
            if (callbackRegistered) runCatching { connectivity.unregisterNetworkCallback(callback) }
            if (Build.VERSION.SDK_INT >= 27) runCatching { p2pChannel?.close() }
            thread.quitSafely()
        }
    }
    private companion object { const val TETHER = "android.net.conn.TETHER_STATE_CHANGED" }
}
