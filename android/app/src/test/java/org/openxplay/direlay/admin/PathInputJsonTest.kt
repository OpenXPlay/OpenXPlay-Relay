// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-27
package org.openxplay.direlay.admin

import android.app.Application
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import org.openxplay.direlay.path.*
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import java.lang.reflect.Modifier

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [28], manifest = Config.NONE, application = Application::class)
class PathInputJsonTest {
    private val rich = PathInput(
        nowMillis = 1793000000100, version = "0.2.1",
        setup = SetupInput(NetworkPlan.PHONE_HOTSPOT, linkedSetOf(CarOutput.CARLINK, CarOutput.BROWSER, CarOutput.TESLA),
            "Wireless · Hotspot · Browser", false),
        relay = RelayInput(identity = IdentityState.READY, identityLabel = "Installed", permissionsGranted = 3,
            permissionsTotal = 5, vpnConsentNeeded = true, keepAlive = KeepAliveState.FOREGROUND,
            bluetoothOn = false, bluetoothName = "Relay \"demo\"\n手机", wifiOn = true, wifiSsid = "Garage",
            hotspotOn = true, hotspotSsid = "OpenXPlay-1A2B", hotspotBand = "5 GHz", sharedWifiSsid = "Saved garage", overlayNeeded = true),
        car = CarInput(address = "192.168.43.1", web = WebServerInput(WebPhase.FAILED, 9443, "web_port_busy", "car.example", 1793000000001),
            pageUrl = "https://car.example:9443/", pages = 2, screen = "1920×720", savedScreen = "1280×720",
            carlink = CarlinkInput(phase = CarlinkPhase.PIN, sinceMillis = 1793000000002, code = "auth_rejected", error = "Try again",
                retrying = true, carName = "G6", joinMethod = "Wi-Fi Direct", trusted = true, pinPending = true,
                picture = "1280×720 H.265", trail = listOf("scan", "join", "scan")),
            dlna = OutputInput(OutputPhase.FAILED, "Renderer", "renderer_stopped", 1793000000003),
            miracast = OutputInput(OutputPhase.PLAYING, "Car display", "display_recovered", 1793000000004),
            tesla = OutputInput(OutputPhase.READY, "https://7.0.0.1/", "address_recovered", 1793000000005)),
        iphone = IphoneInput(pick = IphonePick.PICKED, name = "Riko's iPhone", usb = UsbRole.HOST, started = true, armed = true,
            carWaitUntilMillis = 1793000020000, step = LinkStep.JOINING, stepSinceMillis = 1793000000006,
            stepDetail = "Waiting for the iPhone",
            failure = LinkFailure(LinkStep.WIFI_SENT, "first_tcp_timeout", "Join failed", 1793000000007, true,
                listOf("Wi-Fi details sent", "first_tcp_timeout"))),
        live = LiveInput(true, 1793000000008, listOf("1920×720", "60 fps", "H.265")),
    )

    @Test fun richPathInputRoundTripsEverySection() {
        val expected = JSONObject("""{
            "platform": "android", "nowMillis": 1793000000100, "version": "0.2.1",
            "setup": {"network": "PHONE_HOTSPOT", "outputs": ["CARLINK", "BROWSER", "TESLA"],
                "summary": "Wireless · Hotspot · Browser", "valid": false},
            "relay": {"identity": "READY", "identityLabel": "Installed", "permissionsGranted": 3, "permissionsTotal": 5,
                "vpnConsentNeeded": true, "keepAlive": "FOREGROUND", "bluetoothOn": false,
                "bluetoothName": "Relay \"demo\"\n手机", "wifiOn": true, "wifiSsid": "Garage", "hotspotOn": true,
                "hotspotSsid": "OpenXPlay-1A2B", "hotspotBand": "5 GHz", "sharedWifiSsid": "Saved garage", "overlayNeeded": true},
            "car": {"address": "192.168.43.1",
                "web": {"phase": "FAILED", "port": 9443, "code": "web_port_busy", "trustedName": "car.example", "sinceMillis": 1793000000001},
                "pageUrl": "https://car.example:9443/", "pages": 2, "screen": "1920×720", "savedScreen": "1280×720",
                "carlink": {"phase": "PIN", "sinceMillis": 1793000000002, "code": "auth_rejected", "error": "Try again",
                    "retrying": true, "carName": "G6", "joinMethod": "Wi-Fi Direct", "trusted": true, "pinPending": true,
                    "picture": "1280×720 H.265", "trail": ["scan", "join", "scan"]},
                "dlna": {"phase": "FAILED", "name": "Renderer", "code": "renderer_stopped", "sinceMillis": 1793000000003},
                "miracast": {"phase": "PLAYING", "name": "Car display", "code": "display_recovered", "sinceMillis": 1793000000004},
                "tesla": {"phase": "READY", "name": "https://7.0.0.1/", "code": "address_recovered", "sinceMillis": 1793000000005}},
            "iphone": {"pick": "PICKED", "name": "Riko's iPhone", "usb": "HOST", "started": true, "armed": true,
                "carWaitUntilMillis": 1793000020000, "step": "JOINING", "stepSinceMillis": 1793000000006,
                "stepDetail": "Waiting for the iPhone",
                "failure": {"step": "WIFI_SENT", "code": "first_tcp_timeout", "text": "Join failed", "atMillis": 1793000000007,
                    "retrying": true, "trail": ["Wi-Fi details sent", "first_tcp_timeout"]}},
            "live": {"pictureOnCar": true, "sessionSinceMillis": 1793000000008, "chips": ["1920×720", "60 fps", "H.265"]}
        }""")
        val decoded = JSONObject(rich.json().toString())
        assertEquals(readJsonValue(expected), readJsonValue(decoded))
    }

    @Test fun enumValuesUseKotlinNames() {
        for (network in NetworkPlan.entries) assertEquals(network.name,
            rich.copy(setup = rich.setup.copy(network = network)).json().getJSONObject("setup").getString("network"))
        for (output in CarOutput.entries) assertEquals(output.name,
            rich.copy(setup = rich.setup.copy(outputs = setOf(output))).json().getJSONObject("setup").getJSONArray("outputs").getString(0))
        for (identity in IdentityState.entries) assertEquals(identity.name,
            rich.copy(relay = rich.relay.copy(identity = identity)).json().getJSONObject("relay").getString("identity"))
        for (keepAlive in KeepAliveState.entries) assertEquals(keepAlive.name,
            rich.copy(relay = rich.relay.copy(keepAlive = keepAlive)).json().getJSONObject("relay").getString("keepAlive"))
        for (phase in WebPhase.entries) assertEquals(phase.name,
            rich.copy(car = rich.car.copy(web = rich.car.web.copy(phase = phase))).json().getJSONObject("car").getJSONObject("web").getString("phase"))
        for (phase in CarlinkPhase.entries) assertEquals(phase.name,
            rich.copy(car = rich.car.copy(carlink = rich.car.carlink.copy(phase = phase))).json().getJSONObject("car").getJSONObject("carlink").getString("phase"))
        for (phase in OutputPhase.entries) {
            val car = rich.copy(car = rich.car.copy(dlna = rich.car.dlna.copy(phase = phase),
                miracast = rich.car.miracast.copy(phase = phase), tesla = rich.car.tesla.copy(phase = phase))).json().getJSONObject("car")
            for (output in listOf("dlna", "miracast", "tesla")) assertEquals(phase.name, car.getJSONObject(output).getString("phase"))
        }
        for (pick in IphonePick.entries) assertEquals(pick.name,
            rich.copy(iphone = rich.iphone.copy(pick = pick)).json().getJSONObject("iphone").getString("pick"))
        for (usb in UsbRole.entries) assertEquals(usb.name,
            rich.copy(iphone = rich.iphone.copy(usb = usb)).json().getJSONObject("iphone").getString("usb"))
        for (step in LinkStep.entries) {
            val iphone = rich.copy(iphone = rich.iphone.copy(step = step, failure = rich.iphone.failure!!.copy(step = step))).json().getJSONObject("iphone")
            assertEquals(step.name, iphone.getString("step"))
            assertEquals(step.name, iphone.getJSONObject("failure").getString("step"))
        }
    }

    @Test fun unknownValuesAreExplicitJsonNulls() {
        val json = JSONObject(PathInput().json().toString())
        assertEquals("android", json.getString("platform"))
        assertNullFields(json.getJSONObject("relay"), "identityLabel", "bluetoothOn", "bluetoothName", "wifiOn", "wifiSsid",
            "hotspotOn", "hotspotSsid", "hotspotBand", "sharedWifiSsid")
        val car = json.getJSONObject("car")
        assertNullFields(car, "address", "pageUrl", "screen", "savedScreen")
        assertNullFields(car.getJSONObject("web"), "port", "code", "trustedName", "sinceMillis")
        assertNullFields(car.getJSONObject("carlink"), "sinceMillis", "code", "error", "carName", "joinMethod", "picture")
        for (output in listOf("dlna", "miracast", "tesla")) assertNullFields(car.getJSONObject(output), "name", "code", "sinceMillis")
        assertNullFields(json.getJSONObject("iphone"), "name", "carWaitUntilMillis", "stepSinceMillis", "stepDetail", "failure")
        assertNullFields(json.getJSONObject("live"), "sessionSinceMillis")
    }

    @Test fun knownRadioReadingsRemainBooleans() {
        for (enabled in listOf(false, true)) {
            val relay = rich.copy(relay = rich.relay.copy(bluetoothOn = enabled, wifiOn = enabled, hotspotOn = enabled)).json().getJSONObject("relay")
            for (radio in listOf("bluetoothOn", "wifiOn", "hotspotOn")) assertEquals(enabled, relay.get(radio))
        }
    }

    @Test fun emptyCollectionsRemainArrays() {
        val input = rich.copy(setup = rich.setup.copy(outputs = emptySet()),
            car = rich.car.copy(carlink = rich.car.carlink.copy(trail = emptyList())),
            iphone = rich.iphone.copy(failure = rich.iphone.failure!!.copy(trail = emptyList())),
            live = rich.live.copy(chips = emptyList()))
        val json = JSONObject(input.json().toString())
        assertEquals(0, json.getJSONObject("setup").getJSONArray("outputs").length())
        assertEquals(0, json.getJSONObject("car").getJSONObject("carlink").getJSONArray("trail").length())
        assertEquals(0, json.getJSONObject("iphone").getJSONObject("failure").getJSONArray("trail").length())
        assertEquals(0, json.getJSONObject("live").getJSONArray("chips").length())
    }

    @Test fun everyStoredPropertyHasExactlyOneJsonKey() {
        val json = rich.json()
        assertProperties(rich, json, setOf("platform"))
        assertProperties(rich.setup, json.getJSONObject("setup"))
        assertProperties(rich.relay, json.getJSONObject("relay"))
        val car = json.getJSONObject("car")
        assertProperties(rich.car, car)
        assertProperties(rich.car.web, car.getJSONObject("web"))
        assertProperties(rich.car.carlink, car.getJSONObject("carlink"))
        assertProperties(rich.car.dlna, car.getJSONObject("dlna"))
        assertProperties(rich.car.miracast, car.getJSONObject("miracast"))
        assertProperties(rich.car.tesla, car.getJSONObject("tesla"))
        assertProperties(rich.iphone, json.getJSONObject("iphone"))
        assertProperties(rich.iphone.failure!!, json.getJSONObject("iphone").getJSONObject("failure"))
        assertProperties(rich.live, json.getJSONObject("live"))
    }

    private fun assertNullFields(json: JSONObject, vararg keys: String) {
        for (key in keys) {
            assertTrue("Missing null field: $key", json.has(key))
            assertSame(key, JSONObject.NULL, json.get(key))
        }
    }

    private fun assertProperties(input: Any, json: JSONObject, extraKeys: Set<String> = emptySet()) {
        // Java fields cover stored data-class properties without kotlin-reflect; derived getters have no field.
        val properties = input.javaClass.declaredFields.filterNot { Modifier.isStatic(it.modifiers) || it.isSynthetic }.map { it.name }.toSet()
        assertEquals("${input.javaClass.simpleName} properties must match the explicit serializer keys",
            properties + extraKeys, json.keys().asSequence().toSet())
    }
}
