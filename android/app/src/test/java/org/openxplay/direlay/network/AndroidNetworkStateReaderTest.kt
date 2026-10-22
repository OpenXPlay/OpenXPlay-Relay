// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-21
package org.openxplay.direlay.network

import android.app.Application
import android.content.BroadcastReceiver
import android.content.Context
import android.content.ContextWrapper
import android.content.Intent
import android.content.IntentFilter
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [28], application = Application::class)
class AndroidNetworkStateReaderTest {
    private class Broadcasts : ContextWrapper(RuntimeEnvironment.getApplication()) {
        val intents = mutableMapOf<String, Intent>()
        var denied = false
        override fun getApplicationContext(): Context = this
        override fun registerReceiver(receiver: BroadcastReceiver?, filter: IntentFilter): Intent? {
            if (denied) throw SecurityException()
            assertNull(receiver)
            return intents[filter.getAction(0)]
        }
    }

    @Test fun stickyUsbExtrasReachThePureRoleMapperAndMissingExtrasStayUnknown() {
        val context = Broadcasts()
        val reader = AndroidNetworkStateReader(context)
        assertEquals(UsbConnectionState.UNKNOWN, reader.read().usb)
        val intent = Intent("android.hardware.usb.action.USB_STATE")
        context.intents[intent.action!!] = intent
        assertEquals(UsbConnectionState.UNKNOWN, reader.read().usb)
        intent.putExtra("connected", true)
        assertEquals(UsbConnectionState.UNKNOWN, reader.read().usb)
        intent.putExtra("host_connected", false)
        assertEquals(UsbConnectionState.DEVICE, reader.read().usb)
        intent.putExtra("host_connected", true)
        assertEquals(UsbConnectionState.HOST, reader.read().usb)
        intent.putExtra("connected", false).putExtra("host_connected", false)
        assertEquals(UsbConnectionState.DISCONNECTED, reader.read().usb)
        context.denied = true
        assertEquals(UsbConnectionState.UNKNOWN, reader.read().usb)
    }

    @Test fun activeTetherListsSupportBothAndroidKeysAndDoNotMistakeUsbForWifi() {
        val context = Broadcasts()
        val reader = AndroidNetworkStateReader(context)
        assertNull(reader.hotspotEnabled())
        for (key in listOf("tetherArray", "activeArray")) {
            val intent = Intent("android.net.conn.TETHER_STATE_CHANGED")
            context.intents[intent.action!!] = intent
            assertNull(reader.hotspotEnabled())
            intent.putStringArrayListExtra(key, arrayListOf("wlan1", "rndis0"))
            assertEquals(true, reader.hotspotEnabled())
            intent.putStringArrayListExtra(key, arrayListOf("rndis0"))
            assertEquals(false, reader.hotspotEnabled())
            intent.putStringArrayListExtra(key, arrayListOf())
            assertEquals(false, reader.hotspotEnabled())
            intent.putStringArrayListExtra(key, arrayListOf("vendor_unknown0"))
            assertNull(reader.hotspotEnabled())
        }
        context.denied = true
        assertNull(reader.hotspotEnabled())
    }
}
