// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-22
package org.openxplay.direlay.service

import android.Manifest
import android.app.Application
import android.app.Notification
import android.app.NotificationManager
import android.content.ComponentName
import android.content.pm.PackageManager
import android.content.pm.ServiceInfo
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import org.openxplay.direlay.RelayActivity
import org.robolectric.Robolectric
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [34], qualifiers = "en", application = Application::class)
class RelayKeepAliveServiceTest {
    private val context = RuntimeEnvironment.getApplication()

    @Suppress("DEPRECATION")
    @Test fun mergedManifestDeclaresTheConnectedDeviceServiceAndPermissions() {
        val manager = context.packageManager
        val service = manager.getServiceInfo(ComponentName(context, RelayKeepAliveService::class.java), 0)
        assertFalse(service.exported)
        assertEquals(ServiceInfo.FOREGROUND_SERVICE_TYPE_CONNECTED_DEVICE, service.foregroundServiceType)
        val permissions = manager.getPackageInfo(context.packageName, PackageManager.GET_PERMISSIONS).requestedPermissions.orEmpty()
        assertTrue(Manifest.permission.FOREGROUND_SERVICE in permissions)
        assertTrue(Manifest.permission.FOREGROUND_SERVICE_CONNECTED_DEVICE in permissions)
        assertTrue(Manifest.permission.CHANGE_WIFI_STATE in permissions)
    }

    @Test fun notificationIsSilentOngoingAndOpensTheRelay() {
        val service = Robolectric.buildService(RelayKeepAliveService::class.java).create()
        try {
            val notification = service.get().notification()
            assertEquals("relay_keep_alive", notification.channelId)
            assertTrue(notification.flags and Notification.FLAG_ONGOING_EVENT != 0)
            assertEquals(Notification.CATEGORY_SERVICE, notification.category)
            assertEquals("OpenXPlay Relay", shadowOf(notification).contentTitle)
            assertEquals("Keeps Carlink and the head-unit page working with the screen off.", shadowOf(notification).contentText)
            assertEquals(ComponentName(context, RelayActivity::class.java), shadowOf(notification.contentIntent).savedIntent.component)
            assertTrue(notification.contentIntent.isImmutable)
            assertTrue(notification.actions.isNullOrEmpty())
            val channel = context.getSystemService(NotificationManager::class.java).getNotificationChannel(notification.channelId)
            assertEquals("Relay running", channel.name.toString())
            assertEquals(NotificationManager.IMPORTANCE_MIN, channel.importance)
            assertNull(channel.sound)
            assertFalse(channel.canShowBadge())
        } finally { service.destroy() }
    }

    @Config(qualifiers = "zh-rCN")
    @Test fun notificationUsesSimplifiedChineseResources() {
        val service = Robolectric.buildService(RelayKeepAliveService::class.java).create()
        try {
            val notification = service.get().notification()
            assertEquals("熄屏时保持 Carlink 和车机网页工作。", shadowOf(notification).contentText)
            val channel = context.getSystemService(NotificationManager::class.java).getNotificationChannel(notification.channelId)
            assertEquals("中继运行状态", channel.name.toString())
        } finally { service.destroy() }
    }
}
