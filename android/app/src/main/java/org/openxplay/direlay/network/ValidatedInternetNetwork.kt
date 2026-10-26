// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-25
package org.openxplay.direlay.network

import android.net.ConnectivityManager
import android.net.Network
import android.net.NetworkCapabilities

@Suppress("DEPRECATION")
fun validatedInternetNetwork(connectivity: ConnectivityManager): Network? {
    fun validated(network: Network) = connectivity.getNetworkCapabilities(network)?.let {
        it.hasCapability(NetworkCapabilities.NET_CAPABILITY_INTERNET) && it.hasCapability(NetworkCapabilities.NET_CAPABILITY_VALIDATED)
    } == true
    return connectivity.activeNetwork?.takeIf(::validated) ?: connectivity.allNetworks.firstOrNull(::validated)
}
