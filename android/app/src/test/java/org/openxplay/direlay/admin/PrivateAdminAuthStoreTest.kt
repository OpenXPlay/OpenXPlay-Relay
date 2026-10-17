// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-15
package org.openxplay.direlay.admin

import android.app.Application
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [28], manifest = Config.NONE, application = Application::class)
class PrivateAdminAuthStoreTest {
    @Test fun passwordAndHashedSessionSurviveStoreRecreationAndReset() {
        val context = RuntimeEnvironment.getApplication()
        context.getSharedPreferences("direlay-admin-auth", 0).edit().clear().commit()
        val first = AdminAuth(PrivateAdminAuthStore(context))
        first.changePassword("a", "openxplay", "new-password")
        val token = first.login("a", "new-password").token!!
        val second = AdminAuth(PrivateAdminAuthStore(context))
        assertFalse(second.defaultPassword)
        assertTrue(second.validSession(token))
        assertFalse(context.getSharedPreferences("direlay-admin-auth", 0).getString("state", "")!!.contains(token))
        second.reset()
        assertFalse(first.validSession(token))
        assertTrue(first.defaultPassword)
    }
}
