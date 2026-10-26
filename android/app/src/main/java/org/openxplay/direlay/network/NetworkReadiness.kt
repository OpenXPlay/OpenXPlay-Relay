// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-21
package org.openxplay.direlay.network

enum class Readiness { OK, PROBLEM, UNKNOWN }
enum class NetworkRequirement { BLUETOOTH, WIFI, HOTSPOT, SHARED_WIFI }
enum class UsbConnectionState { DEVICE, HOST, DISCONNECTED, UNKNOWN }

interface NetworkStateReader {
    fun bluetoothEnabled(): Boolean? = null
    fun wifiEnabled(): Boolean?
    fun hotspotEnabled(): Boolean?
}

data class NetworkReadings(val wifi: Boolean? = null, val hotspot: Boolean? = null,
    val usb: UsbConnectionState = UsbConnectionState.UNKNOWN, val usbDevicesEmpty: Boolean? = null, val bluetooth: Boolean? = null) : NetworkStateReader {
    override fun bluetoothEnabled() = bluetooth
    override fun wifiEnabled() = wifi
    override fun hotspotEnabled() = hotspot
}

/** An irrelevant check is OK, even if Android cannot read that radio's state. */
data class NetworkReadiness(val wifi: Readiness = Readiness.OK, val hotspot: Readiness = Readiness.OK,
    val bluetooth: Readiness = Readiness.OK, val sharedWifi: Readiness = Readiness.OK) {
    val problems: Set<NetworkRequirement> get() = matching(Readiness.PROBLEM)
    val unknown: Set<NetworkRequirement> get() = matching(Readiness.UNKNOWN)
    private fun matching(state: Readiness) = buildSet {
        if (bluetooth == state) add(NetworkRequirement.BLUETOOTH)
        if (wifi == state) add(NetworkRequirement.WIFI)
        if (hotspot == state) add(NetworkRequirement.HOTSPOT)
        if (sharedWifi == state) add(NetworkRequirement.SHARED_WIFI)
    }
    companion object {
        fun read(carlink: Boolean, wireless: Boolean, phoneHotspot: Boolean, reader: NetworkStateReader,
            miracast: Boolean = false, wifiDirect: Boolean = false, sharedWifi: Boolean = false, sharedWifiJoined: Boolean = true): NetworkReadiness {
            fun check(required: Boolean, read: () -> Boolean?): Readiness {
                if (!required) return Readiness.OK
                return when (runCatching(read).getOrNull()) {
                    true -> Readiness.OK
                    false -> Readiness.PROBLEM
                    null -> Readiness.UNKNOWN
                }
            }
            return NetworkReadiness(check(carlink || miracast || wireless && (wifiDirect || sharedWifi), reader::wifiEnabled),
                check(wireless && phoneHotspot, reader::hotspotEnabled),
                check(wireless || carlink, reader::bluetoothEnabled),
                check(wireless && sharedWifi) { sharedWifiJoined })
        }
    }
}

/** Missing extras are unknown; host evidence takes precedence on dual-role devices. */
fun usbConnectionState(connected: Boolean?, hostConnected: Boolean?): UsbConnectionState = when {
    hostConnected == true -> UsbConnectionState.HOST
    connected == true && hostConnected == false -> UsbConnectionState.DEVICE
    connected == false && hostConnected == false -> UsbConnectionState.DISCONNECTED
    else -> UsbConnectionState.UNKNOWN
}

/** Never infer a role from the cable or from an unreadable USB device list. */
fun wiredUsbRoleProblem(wireless: Boolean, waiting: Boolean, readings: NetworkReadings): Boolean =
    !wireless && waiting && readings.usbDevicesEmpty == true && readings.usb == UsbConnectionState.DEVICE

/** Tether broadcasts also list USB/Bluetooth interfaces; only known Wi-Fi names count. */
fun wifiTetheringEnabled(active: List<String>?): Boolean? {
    if (active == null) return null
    fun wifi(name: String) = name.matches(Regex("(?:wlan|ap|softap|swlan|wifi)\\d+(?:[._-].*)?"))
    fun other(name: String) = name.matches(Regex("(?:usb|rndis|bt-pan|bnep|eth|ncm)\\d*"))
    return when {
        active.any(::wifi) -> true
        active.all(::other) -> false
        else -> null
    }
}
