// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-11-01
package org.openxplay.direlay.bluetooth

import android.Manifest
import android.app.Application
import android.content.pm.PackageManager
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [31, 33], application = Application::class)
class BluetoothPermissionsTest {
    @Suppress("DEPRECATION")
    @Test fun mergedManifestKeepsLegacySocketPermissionsOnAndroid12AndLater() {
        val context = RuntimeEnvironment.getApplication()
        val manager = context.packageManager
        val permissions = manager.getPackageInfo(context.packageName, PackageManager.GET_PERMISSIONS)
            .requestedPermissions.orEmpty()

        for (permission in listOf(Manifest.permission.BLUETOOTH, Manifest.permission.BLUETOOTH_ADMIN)) {
            assertTrue("$permission must remain available to OEM Bluetooth stacks", permission in permissions)
            assertEquals(permission, PackageManager.PERMISSION_GRANTED,
                manager.checkPermission(permission, context.packageName))
        }
        for (permission in listOf(Manifest.permission.BLUETOOTH_CONNECT, Manifest.permission.BLUETOOTH_SCAN,
                Manifest.permission.BLUETOOTH_ADVERTISE)) {
            assertTrue("$permission must remain declared for modern Bluetooth access", permission in permissions)
        }
    }
}
