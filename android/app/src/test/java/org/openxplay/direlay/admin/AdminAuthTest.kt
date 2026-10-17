// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-15
package org.openxplay.direlay.admin

import org.junit.Assert.*
import org.junit.Test

internal class MemoryAdminAuthStore : AdminAuthStore {
    var state = AdminAuthState()
    override fun read() = state
    override fun write(state: AdminAuthState) { this.state = state }
}
class AdminAuthTest {
    @Test fun defaultPasswordAndWrongPassword() {
        val auth = AdminAuth(MemoryAdminAuthStore())
        assertTrue(auth.defaultPassword)
        assertEquals(AdminAuth.Failure.PASSWORD, auth.login("a", "wrong").failure)
        assertNotNull(auth.login("a", "openxplay").token)
    }
    @Test fun changeRequiresCurrentPasswordAndEightCharactersAndRevokesSessions() {
        val store = MemoryAdminAuthStore()
        val auth = AdminAuth(store)
        val old = auth.login("a", "openxplay").token!!
        assertEquals(AdminAuth.Failure.PASSWORD, auth.changePassword("a", "wrong", "new-password"))
        assertEquals(AdminAuth.Failure.FORMAT, auth.changePassword("a", "openxplay", "short"))
        assertNull(auth.changePassword("a", "openxplay", "new-password"))
        assertFalse(auth.defaultPassword)
        assertFalse(auth.validSession(old))
        assertEquals(16, java.util.Base64.getUrlDecoder().decode(store.state.password!!.salt).size)
        assertEquals(AdminAuth.Failure.PASSWORD, auth.login("a", "openxplay").failure)
        assertNotNull(auth.login("a", "new-password").token)
        auth.reset()
        assertTrue(auth.defaultPassword)
        assertTrue(store.state.sessions.isEmpty())
    }
    @Test fun fifthFailureLimitsOnlyItsAddressForOneMinute() {
        var now = 100_000L
        val auth = AdminAuth(MemoryAdminAuthStore(), AdminClock { now })
        repeat(4) { assertEquals(AdminAuth.Failure.PASSWORD, auth.login("a", "bad").failure) }
        assertEquals(AdminAuth.Failure.RATE_LIMIT, auth.login("a", "bad").failure)
        assertEquals(AdminAuth.Failure.RATE_LIMIT, auth.login("a", "openxplay").failure)
        assertNotNull(auth.login("b", "openxplay").token)
        now += 60_000
        assertNotNull(auth.login("a", "openxplay").token)
    }
    @Test fun sessionPersistsHashedAcrossInstancesExpiresAndCanBeLoggedOut() {
        var now = 100_000L
        val store = MemoryAdminAuthStore()
        val clock = AdminClock { now }
        val first = AdminAuth(store, clock)
        val token = first.login("a", "openxplay").token!!
        assertEquals(32, java.util.Base64.getUrlDecoder().decode(token).size)
        assertFalse(store.state.sessions.containsKey(token))
        val second = AdminAuth(store, clock)
        assertTrue(second.validSession(token))
        now += AdminAuth.SESSION_MILLIS - 1
        assertTrue(second.validSession(token))
        now++
        assertFalse(second.validSession(token))
        val next = second.login("a", "openxplay").token!!
        second.logout(next)
        assertFalse(first.validSession(next))
        assertFalse(first.validSession("invalid"))
    }
}
