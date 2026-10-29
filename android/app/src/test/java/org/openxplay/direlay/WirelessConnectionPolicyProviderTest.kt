// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-29
package org.openxplay.direlay

import android.app.Application
import com.shilapi.xcertplay.orchestration.WirelessConnectionPolicyProvider
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import org.openxplay.direlay.relay.RelayRuntime
import org.openxplay.direlay.settings.ExperimentalOptions
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [33], application = Application::class)
class WirelessConnectionPolicyProviderTest {
    @Test fun handoffPoliciesStayOffUntilTheRuntimeIsReady() {
        val provider: WirelessConnectionPolicyProvider = DiRelayApplication()
        assertFalse(provider.iapChannelOnlyWhenOffered)
        assertFalse(provider.releaseBluetoothOnHandoffRequest)
        assertFalse(provider.awaitIphoneReconnect)
    }

    @Test fun applicationMapsEachRememberedHandoffChoiceThroughTheMaster() {
        val runtime = RelayRuntime(RuntimeEnvironment.getApplication())
        try {
            runtime.settingsStore.awaitSnapshot()
            val application = DiRelayApplication()
            // Reuse the runtime without starting a second Application's services.
            DiRelayApplication::class.java.getDeclaredField("runtime").apply { isAccessible = true }
                .set(application, runtime)
            val provider: WirelessConnectionPolicyProvider = application
            assertTrue(provider.iapChannelOnlyWhenOffered && provider.releaseBluetoothOnHandoffRequest && provider.awaitIphoneReconnect)
            for (active in listOf(false, true)) for (bits in 0..7) {
                val options = ExperimentalOptions(active = active, iapChannelWhenOffered = bits and 1 != 0,
                    releaseBluetoothOnRequest = bits and 2 != 0, awaitIphoneReconnect = bits and 4 != 0)
                runtime.settingsStore.save(runtime.settingsStore.awaitSnapshot().copy(experimental = options))
                assertEquals(active && options.iapChannelWhenOffered, provider.iapChannelOnlyWhenOffered)
                assertEquals(active && options.releaseBluetoothOnRequest, provider.releaseBluetoothOnHandoffRequest)
                assertEquals(active && options.awaitIphoneReconnect, provider.awaitIphoneReconnect)
            }
        } finally { runtime.close() }
    }
}
