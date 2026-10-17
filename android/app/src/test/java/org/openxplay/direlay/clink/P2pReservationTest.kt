// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-16
package org.openxplay.direlay.clink

import android.app.Application
import com.shilapi.xcertplay.AirPlayPersistence
import com.shilapi.xcertplay.orchestration.WirelessHotspotMode
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import org.openxplay.direlay.settings.RelaySettingsStore
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [28], manifest = Config.NONE, application = Application::class)
class P2pReservationTest {
    @Test fun readsEffectiveDiPlayPreferencesAtEveryJoinEvenWithStaleRelaySnapshot() {
        val context = RuntimeEnvironment.getApplication()
        listOf("direlay", "diplay", "xcertplay_airplay").forEach { context.getSharedPreferences(it, 0).edit().clear().commit() }
        RelaySettingsStore(context).use { store ->
            val stale = store.load().also { store.save(it) }
            val reservation = AirPlayP2pReservation(context)
            for (wireless in listOf(false, true)) for (mode in WirelessHotspotMode.entries) {
                AirPlayPersistence.saveWirelessEnabled(context, wireless)
                AirPlayPersistence.saveWirelessHotspotMode(context, mode)
                assertEquals(wireless && mode == WirelessHotspotMode.WIFI_P2P, reservation.reservedByCarPlay())
                assertEquals(stale, store.snapshot.value)
            }
        }
    }
}
