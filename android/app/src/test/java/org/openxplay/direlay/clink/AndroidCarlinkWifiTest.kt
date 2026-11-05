// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-11-04
package org.openxplay.direlay.clink

import android.app.Application
import android.content.Context
import android.net.*
import android.net.wifi.WifiInfo
import android.net.wifi.WifiManager
import android.net.wifi.WifiNetworkSpecifier
import android.os.Build
import android.os.Handler
import android.os.Looper
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import org.openxplay.stream.clink.ble.HeadUnitReply
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config
import org.robolectric.annotation.LooperMode
import org.robolectric.shadows.ShadowNetwork
import org.robolectric.shadows.ShadowNetworkCapabilities
import org.robolectric.util.ReflectionHelpers
import org.robolectric.util.ReflectionHelpers.ClassParameter
import org.robolectric.shadows.ShadowWifiInfo
import java.net.InetAddress
import java.time.Duration

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [29, 30, 33, 35], application = Application::class)
@LooperMode(LooperMode.Mode.PAUSED)
class AndroidCarlinkWifiTest {
    private val context = RuntimeEnvironment.getApplication()
    private val connectivity = context.getSystemService(ConnectivityManager::class.java)
    private val wifi = context.getSystemService(WifiManager::class.java)
    private val main = shadowOf(Looper.getMainLooper())
    private val events = mutableListOf<String>()
    private val reply = HeadUnitReply("Test car", "car", "test-password", "02:11:22:33:44:55", 5180, 57209, 1002)
    private fun flow() = AndroidCarlinkWifi(context, Handler(Looper.getMainLooper()), reply,
        ClinkDiagnostics { events.add(it.line()) }) { error("Unexpected link loss") }
    private fun publish(callback: ConnectivityManager.NetworkCallback, id: Int, ssid: String = "car") {
        val network = ShadowNetwork.newInstance(id)
        val info = if (Build.VERSION.SDK_INT >= 31) WifiInfo.Builder().setSsid(ssid.toByteArray())
            .setCurrentSecurityType(WifiInfo.SECURITY_TYPE_PSK).build() else ShadowWifiInfo.newInstance().also {
                shadowOf(it).setSSID(ssid)
            }
        val capabilities = ShadowNetworkCapabilities.newInstance().also {
            shadowOf(it).addTransportType(NetworkCapabilities.TRANSPORT_WIFI)
            if (Build.VERSION.SDK_INT >= 31) shadowOf(it).setTransportInfo(info)
            else {
                shadowOf(info).setInetAddress(InetAddress.getByName("192.168.49.2"))
                shadowOf(wifi).setConnectionInfo(info)
            }
        }
        val links = LinkProperties().apply {
            interfaceName = "wlan-test"
            val address = ReflectionHelpers.callConstructor(LinkAddress::class.java,
                ClassParameter.from(InetAddress::class.java, InetAddress.getByName("192.168.49.2")),
                ClassParameter.from(Int::class.javaPrimitiveType, 24))
            ReflectionHelpers.callInstanceMethod<Boolean>(this, "addLinkAddress", ClassParameter.from(LinkAddress::class.java, address))
            if (Build.VERSION.SDK_INT >= 30) dhcpServerAddress = InetAddress.getByName("192.168.49.1") as java.net.Inet4Address
        }
        if (Build.VERSION.SDK_INT == 29) shadowOf(wifi).setDhcpInfo(DhcpInfo().apply {
            ipAddress = 0x0231a8c0; serverAddress = 0x0131a8c0
        })
        // OEM callback order may vary; properties must not be attributed to another Network.
        callback.onLinkPropertiesChanged(network, links)
        callback.onCapabilitiesChanged(network, capabilities)
    }
    @Test fun reusesAnIdentifiedSystemNetworkAndUnregistersWithoutReplacingIt() {
        flow().use { flow ->
            flow.start()
            val observer = shadowOf(connectivity).networkCallbacks.single()
            publish(observer, 101)
            main.idleFor(Duration.ofMillis(250))
            assertEquals(ShadowNetwork.newInstance(101), flow.result.getNow(null)?.network)
            assertEquals(1, shadowOf(connectivity).networkCallbacks.size)
            assertFalse(events.any { it.contains("request_started") })
        }
        assertTrue(shadowOf(connectivity).networkCallbacks.isEmpty())
    }
    @Test fun failedAutomaticRequestLeavesObservationAliveForManualConnection() {
        flow().use { flow ->
            flow.start()
            val observer = shadowOf(connectivity).networkCallbacks.single()
            main.idleFor(Duration.ofMillis(250))
            val request = (shadowOf(connectivity).networkCallbacks - observer).single()
            request.onUnavailable()
            assertTrue(events.contains("wifi_manual_switch"))
            assertFalse(flow.result.isDone)
            assertEquals(setOf(observer), shadowOf(connectivity).networkCallbacks)
            publish(observer, 102)
            assertEquals(ShadowNetwork.newInstance(102), flow.result.getNow(null)?.network)
        }
        assertTrue(shadowOf(connectivity).networkCallbacks.isEmpty())
    }
    @Test fun differentSystemWifiIsNeverUsedForTheCar() {
        flow().use { flow ->
            flow.start()
            publish(shadowOf(connectivity).networkCallbacks.single(), 101, "home")
            main.idleFor(Duration.ofMillis(250))
            assertFalse(flow.result.isDone)
            assertTrue(events.any { it.contains("match=different") })
            assertTrue(events.any { it.contains("request_started") })
        }
    }
    @Test fun unknownSsidAndQuotesAreNormalizedWithoutDestroyingSignificantSpaces() {
        assertNull(AndroidCarlinkWifi.normalizeSsid(WifiManager.UNKNOWN_SSID))
        assertNull(AndroidCarlinkWifi.normalizeSsid("\"${WifiManager.UNKNOWN_SSID}\""))
        assertNull(AndroidCarlinkWifi.normalizeSsid("\"\""))
        assertEquals(" car ", AndroidCarlinkWifi.normalizeSsid("\" car \""))
    }
    @Test @Config(sdk = [35]) fun systemFailureReasonsAreRecordedWithoutCredentials() {
        flow().use { flow ->
            flow.start(); main.idleFor(Duration.ofMillis(250))
            val specifier = WifiNetworkSpecifier.Builder().setSsid("car").setWpa2Passphrase("test-password").build()
            shadowOf(wifi).triggerLocalConnectionFailure(specifier, WifiManager.STATUS_LOCAL_ONLY_CONNECTION_FAILURE_AUTHENTICATION)
            main.idle()
            assertTrue(events.toString(), events.any { it == "wifi_recovery event=failure_authentication" })
            assertTrue(events.none { it.contains("test-password") })
        }
    }
}
