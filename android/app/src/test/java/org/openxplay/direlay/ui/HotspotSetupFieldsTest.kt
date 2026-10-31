// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-31
package org.openxplay.direlay.ui

import android.app.Application
import android.content.ClipboardManager
import com.shilapi.xcertplay.orchestration.MfiTarget
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import org.openxplay.direlay.settings.*
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [33], manifest = Config.NONE, application = Application::class)
class HotspotSetupFieldsTest {
    @Test fun changingOnlyTheSetupMethodDoesNotRequestAReconnect() {
        val before = RelaySettings(ConnectionMode.WIRELESS, MfiTarget.LOCAL, "", "", "", null,
            autoStartOnBoot = false, autoStartWhenReady = false, localPreview = false, webPort = null,
            wirelessNetwork = WirelessNetworkSettings(manualSsid = "My hotspot", manualPassphrase = "my password"))
        val after = before.copy(wirelessNetwork = before.wirelessNetwork.copy(hotspotSetupMethod = HotspotSetupMethod.COPY_TO_SYSTEM))
        assertEquals(SettingsNotice.SILENT, settingsNotice(before, after, carlinkConnected = true))
        assertNull(settingsFeedback(before, after, carlinkConnected = true, running = true))
    }

    @Test fun passwordCopyKeepsItsExactValueAndSuppressesClipboardPreview() {
        val context = RuntimeEnvironment.getApplication()
        val clipboard = context.getSystemService(ClipboardManager::class.java)
        copyHotspotValue(context, "Password", "ABCDEFGH23456789", true)
        assertEquals("ABCDEFGH23456789", clipboard.primaryClip?.getItemAt(0)?.text.toString())
        assertTrue(clipboard.primaryClipDescription?.extras?.getBoolean("android.content.extra.IS_SENSITIVE") == true)
        copyHotspotValue(context, "Name", "OpenXPlay-A-ABCDE", false)
        assertEquals("OpenXPlay-A-ABCDE", clipboard.primaryClip?.getItemAt(0)?.text.toString())
        assertNull(clipboard.primaryClipDescription?.extras)
    }
}
