// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-16
package org.openxplay.direlay.admin

import android.Manifest
import android.app.Application
import android.app.NotificationManager
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import org.openxplay.direlay.R
import org.openxplay.direlay.RelayActivity
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config
import java.util.Locale

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [33], qualifiers = "en", application = Application::class)
class AndroidBluetoothVisibilityTest {
    private val context = RuntimeEnvironment.getApplication()
    private val manager = context.getSystemService(NotificationManager::class.java)
    private var foreground = false
    private val visibility = AndroidBluetoothVisibility(context) { foreground }

    @Test fun foregroundRequestsShowOnTheRelayScreenWithoutANotification() {
        foreground = true
        assertTrue(visibility.prompt())
        assertTrue(shadowOf(manager).allNotifications.isEmpty())
    }

    @Test fun backgroundRequestsPostANotificationThatIsRemovedWhenTaken() {
        assertFalse(visibility.prompt())
        val posted = shadowOf(manager).allNotifications.single()
        assertEquals("pairing", posted.channelId)
        assertEquals("Bluetooth pairing is waiting", shadowOf(posted).contentTitle)
        assertEquals("Tap to make this device visible to your iPhone.", shadowOf(posted).contentText)
        assertNotNull(posted.contentIntent)
        assertEquals(RelayActivity.intent(context).flags, shadowOf(posted.contentIntent).savedIntent.flags)
        assertEquals(PairingWindow.SECONDS * 1000L, posted.timeoutAfter)
        visibility.withdraw()
        assertTrue(shadowOf(manager).allNotifications.isEmpty())
    }

    @Test fun blockedNotificationsFailWithAReasonInsteadOfSilently() {
        shadowOf(manager).setNotificationsEnabled(false)
        val failure = assertThrows(PairingUnavailable::class.java) { visibility.prompt() }
        assertEquals("error.pairingOpenApp", failure.key)
        assertTrue(shadowOf(manager).allNotifications.isEmpty())
    }

    @Test fun theDialogWaitsForNearbyDevicesOnAndroid12AndLater() {
        assertFalse(visibility.dialogAllowed())
        shadowOf(context).grantPermissions(Manifest.permission.BLUETOOTH_CONNECT)
        assertFalse(visibility.dialogAllowed())
        shadowOf(context).grantPermissions(Manifest.permission.BLUETOOTH_ADVERTISE)
        assertTrue(visibility.dialogAllowed())
    }

    @Config(sdk = [30])
    @Test fun olderAndroidShowsTheDialogWithoutARuntimePermission() {
        assertTrue(visibility.dialogAllowed())
    }

    @Test fun notificationCopyIsAvailableInEnglishAndSimplifiedChinese() {
        val chinese = android.content.res.Configuration(context.resources.configuration).apply { setLocale(Locale.SIMPLIFIED_CHINESE) }
        val localized = context.createConfigurationContext(chinese)
        for (key in listOf(R.string.pairing_channel, R.string.pairing_notification_title, R.string.pairing_notification_text)) {
            assertTrue(context.getString(key).isNotBlank())
            assertTrue(localized.getString(key).isNotBlank())
            assertNotEquals(context.getString(key), localized.getString(key))
        }
    }
}
