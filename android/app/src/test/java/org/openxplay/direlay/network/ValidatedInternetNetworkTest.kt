// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-25
package org.openxplay.direlay.network

import android.app.Application
import android.net.ConnectivityManager
import android.net.NetworkCapabilities
import android.net.NetworkInfo
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config
import org.robolectric.shadows.ShadowNetwork
import org.robolectric.shadows.ShadowNetworkCapabilities
import org.robolectric.shadows.ShadowNetworkInfo

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [33], application = Application::class)
@Suppress("DEPRECATION")
class ValidatedInternetNetworkTest {
    private fun info(type: Int) = ShadowNetworkInfo.newInstance(NetworkInfo.DetailedState.CONNECTED, type, 0, true, true)
    private fun capabilities(internet: Boolean, validated: Boolean) = ShadowNetworkCapabilities.newInstance().also {
        val shadow = shadowOf(it)
        shadow.removeCapability(NetworkCapabilities.NET_CAPABILITY_INTERNET)
        shadow.removeCapability(NetworkCapabilities.NET_CAPABILITY_VALIDATED)
        if (internet) shadow.addCapability(NetworkCapabilities.NET_CAPABILITY_INTERNET)
        if (validated) shadow.addCapability(NetworkCapabilities.NET_CAPABILITY_VALIDATED)
    }

    @Test fun validatedActiveNetworkWinsAndCarWifiFallsBackToValidatedMobileData() {
        val connectivity = RuntimeEnvironment.getApplication().getSystemService(ConnectivityManager::class.java)
        val shadow = shadowOf(connectivity)
        shadow.clearAllNetworks()
        shadow.setActiveNetworkInfo(info(ConnectivityManager.TYPE_WIFI))
        shadow.setDefaultNetworkActive(true)
        val wifi = requireNotNull(connectivity.activeNetwork)
        val mobile = ShadowNetwork.newInstance(ConnectivityManager.TYPE_MOBILE)
        shadow.addNetwork(mobile, info(ConnectivityManager.TYPE_MOBILE))
        shadow.setNetworkCapabilities(wifi, capabilities(true, true))
        shadow.setNetworkCapabilities(mobile, capabilities(true, true))
        assertEquals(wifi, validatedInternetNetwork(connectivity))
        shadow.setNetworkCapabilities(wifi, capabilities(true, false))
        assertEquals(mobile, validatedInternetNetwork(connectivity))
        shadow.setNetworkCapabilities(wifi, capabilities(false, true))
        assertEquals(mobile, validatedInternetNetwork(connectivity))
        shadow.setDefaultNetworkActive(false)
        assertEquals(mobile, validatedInternetNetwork(connectivity))
    }
    @Test fun localOrUnvalidatedNetworksDoNotCountAsInternet() {
        val connectivity = RuntimeEnvironment.getApplication().getSystemService(ConnectivityManager::class.java)
        val shadow = shadowOf(connectivity)
        shadow.clearAllNetworks()
        assertNull(validatedInternetNetwork(connectivity))
        val car = ShadowNetwork.newInstance(ConnectivityManager.TYPE_WIFI)
        shadow.addNetwork(car, info(ConnectivityManager.TYPE_WIFI))
        for ((internet, validated) in listOf(false to false, true to false, false to true)) {
            shadow.setNetworkCapabilities(car, capabilities(internet, validated))
            assertNull(validatedInternetNetwork(connectivity))
        }
    }
}
