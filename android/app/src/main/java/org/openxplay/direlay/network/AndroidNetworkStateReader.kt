// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-21
package org.openxplay.direlay.network

import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.hardware.usb.UsbManager
import android.net.wifi.WifiManager

/** Queries sticky system broadcasts without registering a receiver or using privileged APIs. */
class AndroidNetworkStateReader(context: Context) : NetworkStateReader {
    private val context = context.applicationContext
    override fun bluetoothEnabled(): Boolean? = runCatching {
        context.getSystemService(android.bluetooth.BluetoothManager::class.java)?.adapter?.isEnabled
    }.getOrNull()
    override fun wifiEnabled(): Boolean? = readWifiEnabled(context)
    override fun hotspotEnabled(): Boolean? = runCatching {
        val intent = sticky("android.net.conn.TETHER_STATE_CHANGED") ?: return@runCatching null
        // Android releases use these two names for the active downstream interface list.
        val key = listOf("tetherArray", "activeArray").firstOrNull(intent::hasExtra) ?: return@runCatching null
        wifiTetheringEnabled(intent.getStringArrayListExtra(key))
    }.getOrNull()

    fun read(): NetworkReadings {
        val usb = runCatching {
            val intent = sticky("android.hardware.usb.action.USB_STATE")
            fun extra(key: String): Boolean? = if (intent?.hasExtra(key) == true) intent.getBooleanExtra(key, false) else null
            usbConnectionState(extra("connected"), extra("host_connected"))
        }.getOrDefault(UsbConnectionState.UNKNOWN)
        val empty = runCatching { context.getSystemService(UsbManager::class.java)?.deviceList?.isEmpty() }.getOrNull()
        return NetworkReadings(wifiEnabled(), hotspotEnabled(), usb, empty, bluetoothEnabled())
    }

    private fun sticky(action: String): Intent? = context.registerReceiver(null, IntentFilter(action))
}

fun readWifiEnabled(context: Context): Boolean? = runCatching {
    context.applicationContext.getSystemService(WifiManager::class.java)?.isWifiEnabled
}.getOrNull()
