// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-22
package org.openxplay.direlay.service

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.Intent
import android.content.pm.ServiceInfo
import android.os.Build
import android.os.IBinder
import android.util.Log
import org.openxplay.direlay.DiRelayApplication
import org.openxplay.direlay.R
import org.openxplay.direlay.RelayActivity

class RelayKeepAliveService : Service() {
    private var foreground = false
    private var stopReason = "outputs_inactive_or_shutdown"
    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        val keepAlive = (application as DiRelayApplication).keepAlive
        try {
            if (Build.VERSION.SDK_INT >= 29)
                startForeground(NOTIFICATION_ID, notification(), ServiceInfo.FOREGROUND_SERVICE_TYPE_CONNECTED_DEVICE)
            else startForeground(NOTIFICATION_ID, notification())
            keepAlive.foregroundStarted()
            if (!foreground) {
                foreground = true
                runCatching { (application as? DiRelayApplication)?.runtime?.connectionLog?.event("relay", "service_start", "reason=outputs_active") }
            }
        } catch (failure: RuntimeException) {
            Log.w("DiRelayKeepAlive", "Keep-alive foreground start refused", failure)
            stopReason = "foreground_refused"
            stopSelf()
            keepAlive.foregroundRefused()
        }
        return START_NOT_STICKY
    }

    override fun onDestroy() {
        runCatching { (application as? DiRelayApplication)?.runtime?.connectionLog?.event("relay", "service_stop", "reason=$stopReason") }
        super.onDestroy()
    }

    override fun onBind(intent: Intent?): IBinder? = null

    internal fun notification(): Notification {
        val manager = getSystemService(NotificationManager::class.java)
        manager.createNotificationChannel(NotificationChannel(CHANNEL, getString(R.string.relay_keep_alive_channel),
            NotificationManager.IMPORTANCE_MIN).apply { setSound(null, null); setShowBadge(false) })
        val open = PendingIntent.getActivity(this, NOTIFICATION_ID,
            Intent(this, RelayActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_REORDER_TO_FRONT),
            PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT)
        return Notification.Builder(this, CHANNEL)
            .setSmallIcon(R.drawable.ic_diplay_notification)
            .setContentTitle(getString(R.string.relay_keep_alive_title))
            .setContentText(getString(R.string.relay_keep_alive_text))
            .setOngoing(true)
            .setCategory(Notification.CATEGORY_SERVICE)
            .setContentIntent(open)
            .build()
    }

    private companion object {
        const val CHANNEL = "relay_keep_alive"
        const val NOTIFICATION_ID = 4108
    }
}
