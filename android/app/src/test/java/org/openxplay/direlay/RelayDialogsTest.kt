// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-27
package org.openxplay.direlay

import android.app.Activity
import android.app.AlertDialog
import android.app.Application
import android.os.Looper
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.Robolectric
import org.robolectric.RobolectricTestRunner
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [33], manifest = Config.NONE, application = Application::class)
class RelayDialogsTest {
    @Test fun decisionsWaitForResumeAndEachOtherAndReleaseTheirWindowOnDestroy() {
        val controller = Robolectric.buildActivity(Activity::class.java).setup()
        var visible = false
        val host = RelayDialogs { visible = it }
        val first = AlertDialog.Builder(controller.get()).setTitle("first").create()
        val second = AlertDialog.Builder(controller.get()).setTitle("second").create()
        try {
            host.show(first)
            host.show(first)
            host.show(second)
            assertTrue(visible)
            assertFalse(first.isShowing)
            host.active = true
            assertTrue(first.isShowing)
            assertFalse(second.isShowing)
            host.active = false
            first.dismiss()
            shadowOf(Looper.getMainLooper()).idle()
            assertFalse(second.isShowing)
            host.active = true
            assertTrue(second.isShowing)
            host.close()
            shadowOf(Looper.getMainLooper()).idle()
            assertFalse(second.isShowing)
            assertFalse(visible)
            assertFalse(host.busy)
            host.show(first)
            assertFalse(first.isShowing)
        } finally { host.close(); controller.pause().stop().destroy() }
    }
}
