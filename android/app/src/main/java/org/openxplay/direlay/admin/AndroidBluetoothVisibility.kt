// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-16
package org.openxplay.direlay.admin

import android.annotation.SuppressLint
import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.bluetooth.BluetoothAdapter
import android.bluetooth.BluetoothManager
import android.content.Context
import android.content.pm.PackageManager
import android.os.Build
import org.openxplay.direlay.R
import org.openxplay.direlay.RelayActivity

/**
 * The relay screen ([RelayActivity]) shows every pairing prompt, because only an activity can ask for a
 * permission and read the dialog's answer. [showOnScreen] brings it forward while the app is in the
 * foreground and returns false in the background, where a notification opens it instead.
 */
class AndroidBluetoothVisibility(private val context: Context, private val showOnScreen: () -> Boolean) : BluetoothVisibility {
    private val adapter get() = context.getSystemService(BluetoothManager::class.java)?.adapter
    private val notifications get() = context.getSystemService(NotificationManager::class.java)
    override val available: Boolean get() = adapter != null
    override fun dialogAllowed(): Boolean = PairingVisibilityPolicy.dialogAllowed(Build.VERSION.SDK_INT) {
        context.checkSelfPermission(it) == PackageManager.PERMISSION_GRANTED
    }
    override fun prompt(): Boolean {
        val manager = notifications
        return when (PairingVisibilityPolicy.route(showOnScreen(), manager?.areNotificationsEnabled() == true)) {
            PairingVisibilityPolicy.Route.SCREEN -> true
            PairingVisibilityPolicy.Route.NOTIFICATION -> { notify(checkNotNull(manager)); false }
            PairingVisibilityPolicy.Route.NONE -> throw PairingUnavailable("error.pairingOpenApp")
        }
    }
    override fun withdraw() { notifications?.cancel(NOTIFICATION_ID) }
    @SuppressLint("MissingPermission")
    override fun discoverable(): Boolean? = try {
        adapter?.scanMode?.let { it == BluetoothAdapter.SCAN_MODE_CONNECTABLE_DISCOVERABLE }
    } catch (_: SecurityException) { null }

    private fun notify(manager: NotificationManager) {
        manager.createNotificationChannel(NotificationChannel(CHANNEL, context.getString(R.string.pairing_channel),
            NotificationManager.IMPORTANCE_HIGH))
        val open = PendingIntent.getActivity(context, NOTIFICATION_ID,
            RelayActivity.intent(context),
            PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT)
        val notification = Notification.Builder(context, CHANNEL)
            .setSmallIcon(R.drawable.ic_diplay_notification)
            .setContentTitle(context.getString(R.string.pairing_notification_title))
            .setContentText(context.getString(R.string.pairing_notification_text))
            .setCategory(Notification.CATEGORY_REMINDER)
            .setContentIntent(open)
            .setAutoCancel(true)
            .setTimeoutAfter(PairingWindow.SECONDS * 1000L)
            .build()
        try { manager.notify(NOTIFICATION_ID, notification) }
        catch (_: SecurityException) { throw PairingUnavailable("error.pairingOpenApp") }
    }

    private companion object {
        const val CHANNEL = "pairing"
        const val NOTIFICATION_ID = 4107
    }
}
