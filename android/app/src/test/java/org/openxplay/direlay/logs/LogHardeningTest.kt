// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-22
package org.openxplay.direlay.logs

import org.junit.Assert.*
import org.junit.Test
import org.openxplay.direlay.admin.AdminAuth
import org.openxplay.direlay.admin.AdminAuthState
import org.openxplay.direlay.admin.AdminHotspot
import org.openxplay.direlay.admin.AdminPasswordHash
import org.openxplay.direlay.settings.WirelessNetworkSettings

class LogHardeningTest {
    @Test fun secretDataClassesNeverPrintTheirSecretFields() {
        val secret = "do-not-print-this-value"
        val values = listOf(AdminHotspot("MyWiFi", secret, "192.168.1.1", true), AdminAuth.Login(secret),
            AdminAuthState(AdminPasswordHash(secret, secret), mapOf(secret to 123L)), WirelessNetworkSettings(manualPassphrase = secret))
        values.forEach { value -> assertFalse(value.javaClass.simpleName, value.toString().contains(secret)) }
    }
}
