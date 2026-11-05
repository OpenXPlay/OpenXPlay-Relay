// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-11-04
package org.openxplay.direlay.clink

import android.content.Context
import android.annotation.SuppressLint
import android.net.*
import android.net.wifi.WifiInfo
import android.net.wifi.WifiManager
import android.net.wifi.WifiNetworkSpecifier
import android.os.Build
import android.os.Handler
import androidx.annotation.RequiresApi
import org.openxplay.stream.clink.ble.HeadUnitReply
import java.net.Inet4Address

/** Android callbacks remain on one handler; the connection policy is platform independent. */
internal class AndroidCarlinkWifi(context: Context, private val handler: Handler, reply: HeadUnitReply,
    private val diagnostics: ClinkDiagnostics, lost: () -> Unit) : AutoCloseable {
    private val connectivity = context.getSystemService(ConnectivityManager::class.java)
    private val wifi = context.getSystemService(WifiManager::class.java)
    private val driver = object : CarlinkWifiFlow.Driver<Network> {
        override fun observe(observer: CarlinkWifiFlow.Observer<Network>): AutoCloseable {
            val callback = callback(observer)
            connectivity.registerNetworkCallback(NetworkRequest.Builder()
                .addTransportType(NetworkCapabilities.TRANSPORT_WIFI)
                .removeCapability(NetworkCapabilities.NET_CAPABILITY_INTERNET).build(), callback, handler)
            return subscription(callback)
        }
        @RequiresApi(29)
        override fun request(observer: CarlinkWifiFlow.Observer<Network>, unavailable: () -> Unit,
            timeoutMillis: Int): AutoCloseable {
            val callback = callback(observer, unavailable)
            val specifier = WifiNetworkSpecifier.Builder().setSsid(reply.ssid).setWpa2Passphrase(reply.psk).build()
            val request = NetworkRequest.Builder().addTransportType(NetworkCapabilities.TRANSPORT_WIFI)
                .removeCapability(NetworkCapabilities.NET_CAPABILITY_INTERNET).setNetworkSpecifier(specifier).build()
            val failureMonitor = monitorFailures(specifier)
            try { connectivity.requestNetwork(request, callback, handler, timeoutMillis) }
            catch (error: Exception) { failureMonitor.close(); throw error }
            diagnostics.emit(ClinkDiagnostic.WifiRecovery("request_started"))
            val connection = subscription(callback)
            return AutoCloseable { try { connection.close() } finally { failureMonitor.close() } }
        }
        override fun later(millis: Long, action: () -> Unit): AutoCloseable {
            val runnable = Runnable(action)
            handler.postDelayed(runnable, millis)
            return AutoCloseable { handler.removeCallbacks(runnable) }
        }
        /**
         * MIUI on Android 10/11 raises a Wi-Fi picker for every specifier request that can never be
         * answered. One scan makes the phone join the saved head-unit network by itself within about
         * half a second; a refused scan (spent throttle) still leaves a join in progress to wait for.
         */
        @Suppress("DEPRECATION")
        override fun nudgePlatformJoin(): Long {
            if (!miuiConfirmationDialog() || wifi == null || !runCatching { wifi.isWifiEnabled }.getOrDefault(false)) return 0
            val issued = runCatching { wifi.startScan() }.getOrDefault(false)
            diagnostics.emit(ClinkDiagnostic.WifiRecovery(if (issued) "platform_join_scan" else "platform_join_scan_refused"))
            return if (issued) PLATFORM_JOIN_MILLIS else PLATFORM_JOIN_REFUSED_MILLIS
        }
    }
    private val flow = CarlinkWifiFlow(reply.ssid, driver, { phase ->
        when (phase) {
            CarlinkWifiFlow.Phase.AUTOMATIC -> diagnostics.emit(ClinkStage.WIFI_JOIN)
            CarlinkWifiFlow.Phase.MANUAL -> diagnostics.emit(ClinkStage.WIFI_MANUAL)
            CarlinkWifiFlow.Phase.READY -> diagnostics.emit(ClinkDiagnostic.WifiRecovery("network_selected"))
            CarlinkWifiFlow.Phase.CLOSED -> diagnostics.emit(ClinkDiagnostic.WifiRecovery("closed"))
            else -> Unit
        }
    }, { match, endpoints -> diagnostics.emit(ClinkDiagnostic.WifiRecovery(
        "observed", match.name.lowercase(), endpoints)) }, lost)
    val result get() = flow.result
    fun start() { diagnostics.emit(ClinkDiagnostic.WifiRecovery("identifying")); flow.start() }
    fun refresh() {
        runCatching { flow.refresh() }.onFailure {
            diagnostics.emit(ClinkDiagnostic.WifiRecovery("observation_unavailable"))
        }
    }
    override fun close() { flow.close() }

    private fun monitorFailures(specifier: WifiNetworkSpecifier): AutoCloseable {
        if (Build.VERSION.SDK_INT < 34 || wifi == null) return AutoCloseable { }
        var active = true
        val listener = WifiManager.LocalOnlyConnectionFailureListener { failed, reason ->
            if (active && failed == specifier) {
                val cause = when (reason) {
                    WifiManager.STATUS_LOCAL_ONLY_CONNECTION_FAILURE_ASSOCIATION -> "association"
                    WifiManager.STATUS_LOCAL_ONLY_CONNECTION_FAILURE_AUTHENTICATION -> "authentication"
                    WifiManager.STATUS_LOCAL_ONLY_CONNECTION_FAILURE_IP_PROVISIONING -> "ip_provisioning"
                    WifiManager.STATUS_LOCAL_ONLY_CONNECTION_FAILURE_NOT_FOUND -> "not_found"
                    WifiManager.STATUS_LOCAL_ONLY_CONNECTION_FAILURE_NO_RESPONSE -> "no_response"
                    WifiManager.STATUS_LOCAL_ONLY_CONNECTION_FAILURE_USER_REJECT -> "user_reject"
                    else -> "unknown"
                }
                diagnostics.emit(ClinkDiagnostic.WifiRecovery("failure_$cause"))
            }
        }
        try { wifi.addLocalOnlyConnectionFailureListener({ handler.post(it) }, listener) }
        catch (_: Exception) {
            diagnostics.emit(ClinkDiagnostic.WifiRecovery("failure_listener_unavailable"))
            return AutoCloseable { }
        }
        return AutoCloseable {
            active = false
            runCatching { wifi.removeLocalOnlyConnectionFailureListener(listener) }.onFailure {
                diagnostics.emit(ClinkDiagnostic.WifiRecovery("failure_listener_release_failed"))
            }
        }
    }
    private fun subscription(callback: ConnectivityManager.NetworkCallback): AutoCloseable {
        var closed = false
        return AutoCloseable {
            if (!closed) {
                closed = true
                runCatching { connectivity.unregisterNetworkCallback(callback) }.onFailure {
                    diagnostics.emit(ClinkDiagnostic.WifiRecovery("release_failed"))
                }
            }
        }
    }
    private fun callback(observer: CarlinkWifiFlow.Observer<Network>, unavailable: () -> Unit = {}): ConnectivityManager.NetworkCallback {
        // Android 12+ redacts location-sensitive fields unless explicitly requested. A denied
        // permission still produces UNKNOWN and must not be interpreted as a matching SSID.
        val common = object : ConnectivityManager.NetworkCallback() {
            private val wifiNetworks = mutableSetOf<Network>()
            private val infos = mutableMapOf<Network, WifiInfo>()
            private val properties = mutableMapOf<Network, LinkProperties>()
            override fun onCapabilitiesChanged(network: Network, capabilities: NetworkCapabilities) {
                if (!capabilities.hasTransport(NetworkCapabilities.TRANSPORT_WIFI) ||
                    capabilities.hasTransport(NetworkCapabilities.TRANSPORT_VPN)) return
                wifiNetworks.add(network)
                if (Build.VERSION.SDK_INT >= 31) {
                    val info = capabilities.transportInfo as? WifiInfo
                    if (info == null) infos.remove(network) else infos[network] = info
                }
                publish(network)
            }
            override fun onLinkPropertiesChanged(network: Network, linkProperties: LinkProperties) {
                properties[network] = linkProperties
                publish(network)
            }
            override fun onLost(network: Network) {
                wifiNetworks.remove(network); infos.remove(network); properties.remove(network)
                observer.lost(network)
            }
            override fun onUnavailable() { unavailable() }
            private fun publish(network: Network) {
                if (network !in wifiNetworks) return
                val links = properties[network] ?: return
                val local = links.linkAddresses.firstOrNull { it.address is Inet4Address }?.address?.hostAddress
                fun ipv4(value: Int) = (0..3).joinToString(".") { ((value ushr (it * 8)) and 255).toString() }
                // WifiInfo became TransportInfo in Android 12. Earlier releases expose one
                // global WifiInfo, so its address must match this Network before using its SSID.
                @Suppress("DEPRECATION") val info = if (Build.VERSION.SDK_INT >= 31) infos[network]
                    else wifi?.connectionInfo?.takeIf { ipv4(it.ipAddress) == local }
                val peer = if (Build.VERSION.SDK_INT >= 30) links.dhcpServerAddress?.hostAddress else {
                    @Suppress("DEPRECATION") val lease = wifi?.dhcpInfo
                    if (lease == null || lease.serverAddress == 0 || ipv4(lease.ipAddress) != local) null
                    else ipv4(lease.serverAddress)
                }
                @Suppress("DEPRECATION") val ssid = normalizeSsid(info?.ssid)
                val encrypted = if (Build.VERSION.SDK_INT >= 31 && info != null) when (info.currentSecurityType) {
                    WifiInfo.SECURITY_TYPE_PSK, WifiInfo.SECURITY_TYPE_SAE -> true
                    WifiInfo.SECURITY_TYPE_UNKNOWN -> null
                    else -> false
                } else null
                val bssid = info?.bssid?.takeUnless { it == "02:00:00:00:00:00" || it == "00:00:00:00:00:00" }
                observer.changed(CarlinkWifiCandidate(network, ssid, encrypted, peer, local, bssid))
            }
        }
        return if (Build.VERSION.SDK_INT >= 31) object : ConnectivityManager.NetworkCallback(
            ConnectivityManager.NetworkCallback.FLAG_INCLUDE_LOCATION_INFO) {
            override fun onCapabilitiesChanged(network: Network, caps: NetworkCapabilities) = common.onCapabilitiesChanged(network, caps)
            override fun onLinkPropertiesChanged(network: Network, links: LinkProperties) = common.onLinkPropertiesChanged(network, links)
            override fun onLost(network: Network) = common.onLost(network)
            override fun onUnavailable() = common.onUnavailable()
        } else common
    }
    companion object {
        /** Measured on a MI 8 SE: a scan that shows the AP has it joined in 350-430 ms. */
        const val PLATFORM_JOIN_MILLIS = 3_000L
        const val PLATFORM_JOIN_REFUSED_MILLIS = 800L
        internal fun miuiConfirmationDialog(manufacturer: String = Build.MANUFACTURER, sdk: Int = Build.VERSION.SDK_INT) =
            manufacturer.equals("Xiaomi", ignoreCase = true) && sdk <= 30
        // UNKNOWN_SSID is an inlined string, including on Android 10.
        @SuppressLint("InlinedApi")
        internal fun normalizeSsid(value: String?): String? = value?.let {
            if (it.length >= 2 && it.startsWith('"') && it.endsWith('"')) it.substring(1, it.length - 1) else it
        }?.takeUnless {
            it.isBlank() || it == WifiManager.UNKNOWN_SSID
        }
    }
}
