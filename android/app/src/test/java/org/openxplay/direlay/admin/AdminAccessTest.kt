// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-15
package org.openxplay.direlay.admin

import java.net.InetAddress
import org.junit.Assert.*
import org.junit.Test

class AdminAccessTest {
    @Test fun privateAndLocalAddressesAreAllowed() {
        listOf("10.0.0.1", "10.255.255.255", "172.16.0.1", "172.31.255.255", "192.168.1.1", "127.0.0.1",
            "127.254.0.1", "169.254.0.1", "10.78.0.1", "::1", "fe80::1", "febf::1", "fc00::1", "fdff::1")
            .forEach { assertTrue(it, AdminAccess.isPrivate(InetAddress.getByName(it))) }
    }
    @Test fun publicUnspecifiedMulticastAndDeprecatedSiteLocalAreDenied() {
        listOf("0.0.0.0", "8.8.8.8", "100.64.0.1", "172.15.255.255", "172.32.0.1", "192.169.0.1", "169.253.0.1",
            "224.0.0.1", "255.255.255.255", "::", "2001:db8::1", "ff02::1", "fec0::1", "fe7f::1")
            .forEach { assertFalse(it, AdminAccess.isPrivate(InetAddress.getByName(it))) }
    }
    @Test fun mappedIpv4FollowsSameRule() {
        val prefix = ByteArray(16).apply { this[10] = -1; this[11] = -1; this[12] = 10; this[15] = 1 }
        assertTrue(AdminAccess.isPrivate(prefix))
        prefix[12] = 8
        assertFalse(AdminAccess.isPrivate(prefix))
    }
}
