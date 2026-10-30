// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-30
package org.openxplay.direlay.logs

import android.app.ActivityManager
import android.app.usage.UsageStatsManager
import android.bluetooth.BluetoothA2dp
import android.bluetooth.BluetoothAdapter
import android.bluetooth.BluetoothDevice
import android.bluetooth.BluetoothHeadset
import android.bluetooth.BluetoothManager
import android.bluetooth.BluetoothProfile
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.net.ConnectivityManager
import android.net.NetworkCapabilities
import android.net.wifi.WifiManager
import android.os.BatteryManager
import android.os.Build
import android.os.PowerManager
import org.openxplay.direlay.network.AndroidNetworkStateReader

/** Android reads run on the log worker. Broadcasts remember only the selected phone's link state. */
class AndroidConnectionContext(context: Context, private val target: () -> String?,
    private val active: () -> Boolean) : AutoCloseable {
    private val context = context.applicationContext
    @Volatile private var registered = false
    private var selected: String? = null
    private val links = mutableMapOf<String, String>()
    private val receiver = object : BroadcastReceiver() {
        override fun onReceive(context: Context, intent: Intent) {
            runCatching { synchronized(this@AndroidConnectionContext) {
                refreshTarget()
                @Suppress("DEPRECATION") val device = intent.getParcelableExtra<BluetoothDevice>(BluetoothDevice.EXTRA_DEVICE)
                if (selected == null || device?.address != selected) return@synchronized
                when (intent.action) {
                    BluetoothDevice.ACTION_ACL_CONNECTED -> links["btAcl"] = "connected"
                    BluetoothDevice.ACTION_ACL_DISCONNECTED -> links["btAcl"] = "disconnected"
                    BluetoothA2dp.ACTION_CONNECTION_STATE_CHANGED, BluetoothHeadset.ACTION_CONNECTION_STATE_CHANGED -> {
                        val key = if (intent.action == BluetoothA2dp.ACTION_CONNECTION_STATE_CHANGED) "a2dp" else "hfp"
                        links[key] = when (intent.getIntExtra(BluetoothProfile.EXTRA_STATE, -1)) {
                            BluetoothProfile.STATE_CONNECTED -> "connected"; BluetoothProfile.STATE_DISCONNECTED -> "disconnected"
                            BluetoothProfile.STATE_CONNECTING -> "connecting"; BluetoothProfile.STATE_DISCONNECTING -> "disconnecting"
                            else -> "unknown"
                        }
                    }
                }
            } }
        }
    }
    fun start() {
        runCatching {
            val filter = IntentFilter().apply {
                addAction(BluetoothDevice.ACTION_ACL_CONNECTED); addAction(BluetoothDevice.ACTION_ACL_DISCONNECTED)
                addAction(BluetoothA2dp.ACTION_CONNECTION_STATE_CHANGED); addAction(BluetoothHeadset.ACTION_CONNECTION_STATE_CHANGED)
            }
            if (Build.VERSION.SDK_INT >= 33) context.registerReceiver(receiver, filter, Context.RECEIVER_NOT_EXPORTED)
            else context.registerReceiver(receiver, filter)
            registered = true
        }
    }
    @Synchronized private fun refreshTarget() {
        val next = if (active()) target() else null
        if (next != selected) { selected = next; links.clear() }
    }
    @Synchronized private fun link(key: String): String { refreshTarget(); return if (registered) links[key] ?: "unknown" else "unavailable" }
    private fun adapter() = context.getSystemService(BluetoothManager::class.java)?.adapter
    private fun power() = context.getSystemService(PowerManager::class.java)
    private fun battery() = context.registerReceiver(null, IntentFilter(Intent.ACTION_BATTERY_CHANGED))
    fun snapshot() = ConnectionContext(mapOf(
        "wifi" to {
            val connectivity = context.getSystemService(ConnectivityManager::class.java)
            val connected = connectivity?.allNetworks?.any { connectivity.getNetworkCapabilities(it)?.hasTransport(NetworkCapabilities.TRANSPORT_WIFI) == true }
            val wifi = context.getSystemService(WifiManager::class.java)
            when { connected == true -> "connected"; wifi == null -> null; wifi.isWifiEnabled -> "on"; else -> "off" }
        },
        "hotspot" to { AndroidNetworkStateReader(context).hotspotEnabled()?.let { if (it) "on" else "off" } },
        "btAdapter" to { when (adapter()?.state) {
            BluetoothAdapter.STATE_ON -> "on"; BluetoothAdapter.STATE_OFF -> "off"
            BluetoothAdapter.STATE_TURNING_ON -> "turning_on"; BluetoothAdapter.STATE_TURNING_OFF -> "turning_off"; else -> null
        } },
        "btDiscovering" to { adapter()?.isDiscovering?.toString() },
        "btAcl" to { link("btAcl") }, "a2dp" to { link("a2dp") }, "hfp" to { link("hfp") },
        "screen" to { power()?.isInteractive?.let { if (it) "on" else "off" } },
        "powerSave" to { power()?.isPowerSaveMode?.toString() },
        "idle" to { power()?.isDeviceIdleMode?.toString() },
        "standbyBucket" to { if (Build.VERSION.SDK_INT >= 28) context.getSystemService(UsageStatsManager::class.java)?.appStandbyBucket?.let(::standbyBucketName) else null },
        "foreground" to {
            val info = ActivityManager.RunningAppProcessInfo()
            ActivityManager.getMyMemoryState(info)
            (info.importance <= ActivityManager.RunningAppProcessInfo.IMPORTANCE_FOREGROUND_SERVICE).toString()
        },
        "battery" to {
            val state = battery()
            val level = state?.getIntExtra(BatteryManager.EXTRA_LEVEL, -1) ?: -1
            val scale = state?.getIntExtra(BatteryManager.EXTRA_SCALE, -1) ?: -1
            if (level >= 0 && scale > 0) (100L * level / scale).coerceIn(0, 100).toString() else null
        },
        "charging" to { battery()?.getIntExtra(BatteryManager.EXTRA_STATUS, -1)?.takeIf { it >= 0 }
            ?.let { (it == BatteryManager.BATTERY_STATUS_CHARGING || it == BatteryManager.BATTERY_STATUS_FULL).toString() } },
        "thermal" to { if (Build.VERSION.SDK_INT >= 29) when (power()?.currentThermalStatus) {
            0 -> "none"; 1 -> "light"; 2 -> "moderate"; 3 -> "severe"; 4 -> "critical"; 5 -> "emergency"; 6 -> "shutdown"; else -> null
        } else null }))

    override fun close() { if (registered) runCatching { context.unregisterReceiver(receiver) }; registered = false }
}
