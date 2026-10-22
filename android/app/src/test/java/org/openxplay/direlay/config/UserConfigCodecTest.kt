// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-22
package org.openxplay.direlay.config

import android.app.Application
import com.shilapi.xcertplay.orchestration.*
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.openxplay.direlay.settings.*
import org.openxplay.stream.clink.ClinkHevcMode
import org.openxplay.stream.clink.ClinkPictureSize

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [28], manifest = Config.NONE, application = Application::class)
class UserConfigCodecTest {
    private val meta = ConfigMeta("test", "2026-10-22T00:00:00Z")
    private val base = RelaySettings(ConnectionMode.USB, MfiTarget.LOCAL, "", "", "credential-not-exported",
        null, 30, false, false, false, null)
    private fun wrapped(settings: JSONObject) = ConfigEnvelope.create(UserConfigCodec.FORMAT, meta).put("settings", settings).toString()
    private fun decode(settings: JSONObject) = UserConfigCodec.decode(wrapped(settings), base)

    @Test fun populatedRoundTripIncludesAllChoicesWithoutSecrets() {
        val settings = base.copy(connectionMode = ConnectionMode.WIRELESS, mfiTarget = MfiTarget.REMOTE,
            i2cPath = "/dev/i2c-1", remoteServer = "https://mfi.example.org", display = DisplayProfile(1920, 1080, 240),
            fps = 60, autoStartOnBoot = true, autoStartWhenReady = true, localPreview = true, webPort = 8443,
            carlinkOutput = true, bothOutputs = true, carlinkDeviceName = "Relay test", carlinkDeviceModel = "Test model",
            wirelessNetwork = WirelessNetworkSettings(WirelessNetworkChoice.PHONE_HOTSPOT, "Phone", "password1",
                ManualHotspotBand.GHZ_5, 36, ManualHotspotSecurity.WPA2, listOf(SharedWifiNetwork("Home", "password2"))),
            playCarPlayAudio = true, microphoneSource = MicrophoneSource.THIS_DEVICE, vehicleInformation = true,
            carlinkAutoReconnect = false, carlinkReconnectOnResize = true, displayMode = DisplayMode.FIXED,
            iphoneAddress = "12:34:56:78:9A:BC", plugins = true, pluginsEnabled = listOf("test.plugin"),
            carlinkTrace = true, carlinkHevc = ClinkHevcMode.SIGNAL, carlinkPictureSize = ClinkPictureSize.MODE,
            carplayResolution = CarPlayResolution.P720, carplayScreenWidth = CarPlayScreenWidth.MM300, viewAreas = false,
            car = CarSelection("other", "Test brand", "other", "Test car"), videoCodec = VideoCodecSetting.H264,
            videoStats = VideoStatsWindow.SECONDS_15, carBluetoothAudio = false)
        val developer = DeveloperOverrides(active = false, carlinkTrace = true, carlinkHevcEnabled = true,
            carlinkHevc = ClinkHevcMode.OFF, carlinkPictureSizeEnabled = true, carlinkPictureSize = ClinkPictureSize.MODE,
            physicalWidthEnabled = true, physicalWidthOverrideMm = 250, fpsEnabled = true, fps = 55, localPreview = true,
            playCarPlayAudio = true, microphoneSourceEnabled = true, microphoneSource = MicrophoneSource.THIS_DEVICE,
            webPortEnabled = true, webPort = 9443, carlinkDeviceModelEnabled = true, carlinkDeviceModel = "Remembered model")
        val text = UserConfigCodec.encode(settings, developer, meta)
        assertFalse(text.contains(base.remoteToken))
        assertFalse(text.contains("remoteToken"))
        val result = UserConfigCodec.decode(text, base)
        assertTrue(result.errors.toString(), result.errors.isEmpty())
        // Boolean equality keeps credentials out of assertion failure text.
        assertTrue(result.settings == settings)
        assertTrue(result.developer == developer)
        assertTrue(result.changedCount > 30)
        assertEquals(0, UserConfigCodec.decode(text, settings, developer).changedCount)
        val json = JSONObject(text).getJSONObject("settings")
        assertEquals("both", json.getString("output"))
        assertEquals("device-hotspot", json.getString("networkMode"))
        assertEquals("wireless", json.getString("iphoneConnection"))
        assertEquals("720p", json.getString("carplayResolution"))
        assertEquals("mm300", json.getString("carplayScreenWidth"))
        assertTrue(json.getJSONArray("sharedWifiNetworks").getJSONObject(0).has("password"))
        assertFalse(json.has("webPort"))
    }
    @Test fun missingKeysKeepValuesAndLinuxSharedKeysImport() {
        val unchanged = decode(JSONObject())
        assertTrue(unchanged.settings == base)
        assertEquals(0, unchanged.changedCount)
        val sample = JSONObject("""{"output":"both","networkMode":"shared-wifi","sharedWifiNetworks":[{"ssid":"Home","password":"password1"}],"receiverPort":8443,"wifiInterface":"wlan0"}""")
        val imported = decode(sample)
        assertTrue(imported.errors.isEmpty())
        assertTrue(imported.settings.bothOutputs && imported.settings.carlinkOutput)
        assertEquals(WirelessNetworkChoice.EXISTING_WIFI, imported.settings.wirelessNetwork.choice)
        assertEquals(8443, imported.settings.webPort)
        assertEquals(listOf("wifiInterface"), imported.ignoredKeys)
        assertEquals(1, imported.settings.wirelessNetwork.sharedWifiNetworks.size)
    }
    @Test fun invalidValuesRejectEverythingAndUnknownValuesAreReported() {
        val result = decode(JSONObject().put("fps", 31).put("receiverPort", 80).put("autoStartOnBoot", true)
            .put("videoCodec", 1).put("unknown", false).put("hotspotChannel", 999)
            .put("developer", JSONObject().put("unknown", 0)))
        assertTrue(result.errors.keys.containsAll(listOf("fps", "receiverPort", "videoCodec")))
        assertTrue(result.settings == base)
        assertEquals(0, result.changedCount)
        assertEquals(listOf("developer.unknown", "hotspotChannel", "unknown"), result.ignoredKeys)
        for (value in listOf<Any>("false", 1, JSONObject.NULL)) assertTrue(decode(JSONObject().put("plugins", value)).errors.containsKey("plugins"))
    }
    @Test fun envelopesAndUtf8AreStrict() {
        val valid = JSONObject(wrapped(JSONObject()))
        assertEquals("wrong-kind", UserConfigCodec.decode(valid.put("format", PrivacyConfigCodec.FORMAT).toString(), base).errors["format"])
        assertEquals("invalid", UserConfigCodec.decode(valid.put("format", "other").toString(), base).errors["format"])
        valid.put("format", UserConfigCodec.FORMAT).put("version", 2)
        assertEquals("bad-version", UserConfigCodec.decode(valid.toString(), base).errors["version"])
        assertEquals("too-large", UserConfigCodec.decode(ByteArray(65537), base).errors["file"])
        assertEquals("not-json", UserConfigCodec.decode(byteArrayOf(0xc3.toByte(), 0x28), base).errors["file"])
        for (text in listOf("{} trailing", "{unquoted:1}", "{'format':'x'}", "{\"a\":1,}", "{\"a\":1,\"a\":2}"))
            assertEquals("not-json", UserConfigCodec.decode(text, base).errors["file"])
    }
}
