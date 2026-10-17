// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-15
package org.openxplay.direlay.bluetooth

import android.Manifest
import android.annotation.SuppressLint
import android.bluetooth.BluetoothClass
import android.bluetooth.BluetoothDevice
import android.bluetooth.BluetoothManager
import android.content.Context
import android.content.pm.PackageManager
import android.os.Build
import java.util.UUID

/** Reads cached SDP data only; never starts discovery or logs device identity. */
class AndroidBondedDevices(private val context: Context) : BondedDevices {
    private val permitted get() = Build.VERSION.SDK_INT < 31 ||
        context.checkSelfPermission(Manifest.permission.BLUETOOTH_CONNECT) == PackageManager.PERMISSION_GRANTED
    private val adapter get() = context.getSystemService(BluetoothManager::class.java)?.adapter

    @SuppressLint("MissingPermission")
    override fun read(): List<BondedDevice> {
        if (!permitted) return emptyList()
        return runCatching { adapter?.bondedDevices.orEmpty().mapNotNull { device ->
            runCatching {
                val major = device.bluetoothClass?.majorDeviceClass
                BondedDevice(device.address, device.name, major == BluetoothClass.Device.Major.PHONE,
                    device.uuids.orEmpty().any { it.uuid == IAP2 },
                    runCatching { BluetoothDevice::class.java.getMethod("isConnected").invoke(device) == true }.getOrDefault(false),
                    major == BluetoothClass.Device.Major.AUDIO_VIDEO)
            }.getOrNull()
        }.sortedBy { it.address } }.getOrDefault(emptyList())
    }

    @SuppressLint("MissingPermission")
    override fun localName(): String? = if (!permitted) null else runCatching { adapter?.name }.getOrNull()

    private companion object { val IAP2: UUID = UUID.fromString("00000000-deca-fade-deca-deafdecacafe") }
}
