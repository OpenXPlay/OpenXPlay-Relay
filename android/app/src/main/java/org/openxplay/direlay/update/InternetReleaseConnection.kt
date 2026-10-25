// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-25
package org.openxplay.direlay.update

import android.net.ConnectivityManager
import org.openxplay.direlay.network.validatedInternetNetwork
import java.net.HttpURLConnection
import java.net.URL

class InternetReleaseConnection(private val connectivity: ConnectivityManager) : ReleaseConnectionFactory {
    override fun open(url: URL): HttpURLConnection {
        val network = validatedInternetNetwork(connectivity) ?: throw UpdateFailure.NoInternet
        return network.openConnection(url) as HttpURLConnection
    }
}
