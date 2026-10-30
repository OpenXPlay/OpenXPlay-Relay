// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-22
package org.openxplay.direlay.config

import org.openxplay.stream.core.FrameRatePreference
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
    @Test fun displaySettingsRoundTripAndMissingKeysLoadDefaults() {
        for (stage3d in listOf(false, true)) for (highPerformance in listOf(false, true)) {
            val chosen = base.copy(stage3d = stage3d, highPerformance = highPerformance)
            val text = UserConfigCodec.encode(chosen, DeveloperOverrides(), meta)
            val json = JSONObject(text).getJSONObject("settings")
            assertEquals(stage3d, json.getBoolean("stage3d"))
            assertEquals(highPerformance, json.getBoolean("highPerformance"))
            val imported = UserConfigCodec.decode(text, base)
            assertTrue(imported.errors.toString(), imported.errors.isEmpty())
            assertTrue(imported.ignoredKeys.isEmpty())
            assertEquals(chosen, imported.settings)
            assertEquals(0, UserConfigCodec.decode(text, chosen).changedCount)
        }
        val missing = UserConfigCodec.decode(wrapped(JSONObject()), base.copy(stage3d = true, highPerformance = false))
        assertTrue(missing.errors.toString(), missing.errors.isEmpty())
        assertEquals(base, missing.settings)
        assertEquals(2, missing.changedCount)
        assertTrue(decode(JSONObject().put("stage3d", true)).settings.highPerformance)
        assertFalse(decode(JSONObject().put("highPerformance", false)).settings.stage3d)
    }

    @Test fun displaySettingsRequireBooleansAndRejectInvalidImportsAtomically() {
        for (key in listOf("stage3d", "highPerformance")) for (bad in listOf<Any>("true", 1, JSONObject.NULL)) {
            val result = decode(JSONObject().put(key, bad).put("autoStartOnBoot", true))
            assertEquals(mapOf(key to "invalid"), result.errors)
            assertEquals(base, result.settings)
            assertEquals(0, result.changedCount)
        }
    }

    @Test fun frameRateUsesPinnedStringWiresAndExportsNoLegacyFpsKeys() {
        val wires = listOf("auto", "30", "35", "40", "45", "50", "55", "60")
        for ((rate, wire) in FrameRatePreference.entries.zip(wires)) {
            val imported = decode(JSONObject().put("frameRate", wire))
            assertTrue(imported.errors.toString(), imported.errors.isEmpty())
            assertEquals(rate, imported.settings.frameRate)
            val exported = JSONObject(UserConfigCodec.encode(imported.settings, imported.developer, meta)).getJSONObject("settings")
            assertEquals(wire, exported.getString("frameRate"))
            assertFalse(exported.has("fps"))
            assertFalse(exported.getJSONObject("developer").has("fps"))
            assertFalse(exported.getJSONObject("developer").has("fpsEnabled"))
        }
        for (bad in listOf<Any>("AUTO", "FPS_30", "31", 30, JSONObject.NULL))
            assertEquals(mapOf("frameRate" to "invalid"), decode(JSONObject().put("frameRate", bad)).errors)
    }

    @Test fun legacyDeveloperFpsMigratesOnlyWithoutTheUserSettingAndIgnoredKeysAreReported() {
        for (fps in 30..60 step 5) {
            val legacy = JSONObject().put("fpsEnabled", true).put("fps", fps).put("active", false)
            val imported = decode(JSONObject().put("fps", 30).put("developer", legacy))
            assertTrue(imported.errors.toString(), imported.errors.isEmpty())
            assertEquals(FrameRatePreference.valueOf("FPS_$fps"), imported.settings.frameRate)
            assertEquals(listOf("fps"), imported.ignoredKeys)
            val explicit = decode(JSONObject().put("frameRate", "auto").put("developer", legacy))
            assertEquals(FrameRatePreference.AUTO, explicit.settings.frameRate)
            assertEquals(listOf("developer.fps", "developer.fpsEnabled"), explicit.ignoredKeys)
            assertTrue(explicit.errors.isEmpty())
        }
        for (enabled in listOf<Any>(false, "true", JSONObject.NULL)) for (fps in listOf<Any>(31, "60", JSONObject.NULL, 55)) {
            val imported = decode(JSONObject().put("fps", "ignored").put("developer", JSONObject().put("fpsEnabled", enabled).put("fps", fps)))
            assertTrue(imported.errors.toString(), imported.errors.isEmpty())
            assertEquals(base.frameRate, imported.settings.frameRate)
            assertEquals(listOf("developer.fps", "developer.fpsEnabled", "fps"), imported.ignoredKeys)
            assertEquals(0, imported.changedCount)
        }
        for (bad in listOf<Any>(31, "60", JSONObject.NULL)) {
            val imported = decode(JSONObject().put("developer", JSONObject().put("fpsEnabled", true).put("fps", bad)))
            assertTrue(imported.errors.isEmpty())
            assertEquals(base.frameRate, imported.settings.frameRate)
            assertEquals(listOf("developer.fps", "developer.fpsEnabled"), imported.ignoredKeys)
        }
    }

    private val meta = ConfigMeta("test", "2026-10-22T00:00:00Z")
    private val base = RelaySettings(ConnectionMode.USB, MfiTarget.LOCAL, "", "", "credential-not-exported",
        null, FrameRatePreference.AUTO, false, false, false, null)
    private fun wrapped(settings: JSONObject) = ConfigEnvelope.create(UserConfigCodec.FORMAT, meta).put("settings", settings).toString()
    private fun decode(settings: JSONObject) = UserConfigCodec.decode(wrapped(settings), base)

    @Test fun legacyIncompleteCarsImportWithoutBlockingOtherSettings() {
        for (car in listOf(CarSelection(), CarSelection("xpeng"), CarSelection("other", model = "other"),
            CarSelection("xpeng", model = "other"))) {
            val legacy = JSONObject().put("brand", car.brand).put("brandText", car.brandText)
                .put("model", car.model).put("modelText", car.modelText)
            val result = decode(JSONObject().put("car", legacy).put("autoStartOnBoot", true))
            assertTrue(result.errors.toString(), result.errors.isEmpty())
            assertTrue(result.ignoredKeys.toString(), result.ignoredKeys.isEmpty())
            assertEquals(car.normalized(), result.settings.car)
            assertFalse(result.settings.car.complete)
            assertTrue(result.settings.autoStartOnBoot)
            result.settings.validate(wireless = false)
        }
        val current = base.copy(car = CarSelection("xpeng", model = "g6"))
        val missing = UserConfigCodec.decode(wrapped(JSONObject().put("autoStartOnBoot", true)), base)
        assertTrue(missing.errors.isEmpty())
        assertFalse(missing.settings.car.complete)
        val cleared = UserConfigCodec.decode(wrapped(JSONObject().put("car", JSONObject()
            .put("brand", "").put("brandText", "").put("model", "").put("modelText", ""))), current)
        assertTrue(cleared.errors.toString(), cleared.errors.isEmpty())
        assertEquals(CarSelection(), cleared.settings.car)
    }

    @Test fun carYearVersionOneRoundTripsKnownCustomAndIncompleteSelections() {
        for (car in listOf(CarSelection(), CarSelection("xpeng"), CarSelection("xpeng", model = "g6"),
            CarSelection("xpeng", model = "g6", year = "g6-2023"), CarSelection("tesla", model = "model-3", year = "model-3-2024"),
            CarSelection("xpeng", model = "g6", year = "other", yearText = "Custom"),
            CarSelection("xpeng", model = "other", modelText = "Model", year = "other", yearText = "Year"),
            CarSelection("other", "Brand", "other", "Model", "other", "Year"))) {
            val chosen = base.copy(car = car)
            val text = UserConfigCodec.encode(chosen, DeveloperOverrides(), meta)
            val root = JSONObject(text)
            assertEquals(1, root.getInt("version"))
            val encoded = root.getJSONObject("settings").getJSONObject("car")
            assertEquals(car.year, encoded.getString("year")); assertEquals(car.yearText, encoded.getString("yearText"))
            val result = UserConfigCodec.decode(text, base)
            assertTrue(result.errors.toString(), result.errors.isEmpty())
            assertTrue(result.ignoredKeys.toString(), result.ignoredKeys.isEmpty())
            assertEquals(chosen, result.settings)
        }
    }
    @Test fun legacyCarIdsMigrateAndExportsAlwaysWriteTheNewForm() {
        for ((brand, models) in CarSelection.catalogIds) for ((model, years) in models) for (year in years) {
            val legacy = CarSelection(brand, model = year)
            val encoded = JSONObject(UserConfigCodec.encode(base.copy(car = legacy), DeveloperOverrides(), meta)).getJSONObject("settings").getJSONObject("car")
            assertEquals(model, encoded.getString("model")); assertEquals(year, encoded.getString("year"))
            val json = JSONObject().put("brand", brand).put("brandText", "").put("model", year).put("modelText", "")
            val result = decode(JSONObject().put("car", json))
            assertTrue(result.errors.toString(), result.errors.isEmpty())
            assertEquals(CarSelection(brand, model = model, year = year), result.settings.car)
        }
        val custom = JSONObject().put("brand", "other").put("brandText", "Brand").put("model", "other").put("modelText", "Model")
        assertEquals(CarSelection("other", "Brand", "other", "Model", "other"), decode(JSONObject().put("car", custom)).settings.car)
        val modelOnly = JSONObject().put("brand", "xpeng").put("brandText", "").put("model", "g6").put("modelText", "")
        assertEquals(CarSelection("xpeng", model = "g6"), decode(JSONObject().put("car", modelOnly)).settings.car)
        modelOnly.put("future", true)
        assertEquals(listOf("car.future"), decode(JSONObject().put("car", modelOnly)).ignoredKeys)
    }
    @Test fun unknownCarIdsAndMalformedYearFieldsFailAtCarWithoutApplyingOtherChanges() {
        val valid = JSONObject().put("brand", "xpeng").put("brandText", "").put("model", "g6").put("modelText", "").put("year", "g6-2023").put("yearText", "")
        val invalid = listOf(valid.toString().let(::JSONObject).put("brand", "unknown"),
            valid.toString().let(::JSONObject).put("model", "unknown"), valid.toString().let(::JSONObject).put("year", "unknown"),
            valid.toString().let(::JSONObject).put("year", "g9-2024"), valid.toString().let(::JSONObject).put("model", "g6-2023"),
            valid.toString().let(::JSONObject).put("year", 2023), valid.toString().let(::JSONObject).put("yearText", JSONObject.NULL))
        invalid.forEach { car ->
            val result = decode(JSONObject().put("car", car).put("viewAreas", false))
            assertEquals(setOf("car"), result.errors.keys)
            assertEquals(base, result.settings)
        }
    }

    @Test fun wirelessPictureVersionOneRoundTripsAndMissingKeysKeepParkedChoices() {
        for (mode in WirelessPictureMode.entries) for (stretch in listOf(false, true))
            for (size in listOf(WirelessPictureSize(1920, 720), WirelessPictureSize(720, 1280))) {
                val chosen = base.copy(wirelessPicture = mode, wirelessPictureSize = size, miracastStretch = stretch)
                val text = UserConfigCodec.encode(chosen, DeveloperOverrides(), meta)
                val root = JSONObject(text)
                assertEquals(1, root.getInt("version"))
                val json = root.getJSONObject("settings")
                assertEquals(if (mode == WirelessPictureMode.AUTO) "auto" else "custom", json.getString("wirelessPicture"))
                assertEquals(size.width, json.getJSONObject("wirelessPictureSize").getInt("width"))
                assertEquals(size.height, json.getJSONObject("wirelessPictureSize").getInt("height"))
                val result = UserConfigCodec.decode(text, base)
                assertTrue(result.errors.toString(), result.errors.isEmpty())
                assertEquals(chosen, result.settings)
                assertTrue(result.ignoredKeys.isEmpty())
                assertEquals(0, UserConfigCodec.decode(text, chosen).changedCount)
                assertEquals(chosen, UserConfigCodec.decode(wrapped(JSONObject()), chosen).settings)
            }
    }
    @Test fun wirelessPictureInvalidTypesAndOutOfBoundsSizesRejectAtomicallyEvenInAuto() {
        val badSizes = listOf<Any>(JSONObject.NULL, "1280x720", 720, JSONObject(),
            JSONObject().put("width", 1280), JSONObject().put("width", "1280").put("height", 720),
            JSONObject().put("width", 1280.5).put("height", 720)) +
            listOf(1281 to 720, 1280 to 721, 1922 to 1080, 1920 to 1082, 638 to 360, 360 to 638, 0 to 0, -640 to 360)
                .map { (w, h) -> JSONObject().put("width", w).put("height", h) }
        val cases = mapOf("wirelessPictureSize" to badSizes, "wirelessPicture" to listOf<Any>("AUTO", "future", 1, JSONObject.NULL),
            "miracastStretch" to listOf<Any>("true", 1, JSONObject.NULL))
        cases.forEach { (key, values) -> values.forEach { value ->
            val result = decode(JSONObject().put(key, value).put("autoStartOnBoot", true))
            assertEquals(mapOf(key to "invalid"), result.errors)
            assertEquals(base, result.settings); assertEquals(0, result.changedCount)
        } }
        val result = decode(JSONObject().put("wirelessPictureSize", JSONObject().put("width", 1280).put("height", 480).put("future", 1)))
        assertTrue(result.errors.isEmpty()); assertEquals(listOf("wirelessPictureSize.future"), result.ignoredKeys)
    }

    @Test fun wirelessDisplaysRoundTripAtVersionOneAndMissingKeysKeepChoices() {
        for (dlna in listOf(false, true)) for (miracast in listOf(false, true))
            for (picture in DlnaPicture.entries) for (receiver in listOf(null, DlnaRendererChoice("uuid:car", "车机"))) {
                val value = base.copy(outputs = OutputSelection.legacy(false, false, dlna, miracast), dlnaPicture = picture, dlnaRenderer = receiver)
                val text = UserConfigCodec.encode(value, DeveloperOverrides(), meta)
                val root = JSONObject(text)
                assertEquals(1, root.getInt("version"))
                val json = root.getJSONObject("settings")
                assertEquals("web", json.getString("output"))
                assertEquals(if (picture == DlnaPicture.P720) "720p" else "1080p", json.getString("dlnaPicture"))
                assertEquals(receiver == null, json.isNull("dlnaRenderer"))
                val imported = UserConfigCodec.decode(text, base)
                assertTrue(imported.errors.toString(), imported.errors.isEmpty())
                assertEquals(emptyList<String>(), imported.ignoredKeys)
                assertEquals(value, imported.settings)
                assertEquals(0, UserConfigCodec.decode(text, value).changedCount)
                assertEquals(value, UserConfigCodec.decode(wrapped(JSONObject()), value).settings)
            }
        val clear = UserConfigCodec.decode(wrapped(JSONObject().put("dlnaRenderer", JSONObject.NULL)),
            base.copy(dlnaRenderer = DlnaRendererChoice("uuid:old", "Old")))
        assertTrue(clear.errors.isEmpty()); assertNull(clear.settings.dlnaRenderer)
    }
    @Test fun wirelessDisplayInvalidTypesRejectAtomicallyAndNestedUnknownKeysAreReported() {
        val cases = mapOf(
            "dlnaOutput" to listOf<Any>("true", 1, JSONObject.NULL),
            "miracastOutput" to listOf<Any>("false", 0, JSONObject.NULL),
            "dlnaPicture" to listOf<Any>("P720", "4k", 720, JSONObject.NULL),
            "dlnaRenderer" to listOf<Any>("uuid:car", false, org.json.JSONArray(), JSONObject(),
                JSONObject().put("udn", "uuid:car"), JSONObject().put("udn", 1).put("name", "Car"),
                JSONObject().put("udn", "uuid:car").put("name", JSONObject.NULL)))
        cases.forEach { (key, values) -> values.forEach { value ->
            val result = decode(JSONObject().put(key, value).put("autoStartOnBoot", true))
            assertEquals(mapOf(key to "invalid"), result.errors)
            assertEquals(base, result.settings); assertEquals(0, result.changedCount)
        } }
        val result = decode(JSONObject().put("dlnaOutput", true).put("dlnaRenderer",
            JSONObject().put("udn", "uuid:car").put("name", "Car").put("future", 1)))
        assertTrue(result.errors.isEmpty())
        assertEquals(listOf("dlnaRenderer.future"), result.ignoredKeys)
        assertTrue(decode(JSONObject().put("dlnaOutput", true)).errors.isEmpty())
    }

    @Test fun eachSingleOutputAndACombinationRoundTripWithOrderedWiresAndLegacyExport() {
        for (outputs in RelayOutput.entries.map { setOf(it) } + listOf(RelayOutput.entries.reversed().toSet())) {
            val settings = base.copy(outputs = outputs)
            val encoded = UserConfigCodec.encode(settings, DeveloperOverrides(), meta)
            val json = JSONObject(encoded).getJSONObject("settings")
            assertEquals(RelayOutput.entries.filter { it in outputs }.map { it.wire },
                json.getJSONArray("outputs").let { array -> (0 until array.length()).map(array::getString) })
            val legacy = when {
                settings.browserOutput && settings.carlinkOutput -> "both"
                settings.browserOutput -> "web"
                settings.carlinkOutput -> "carlink"
                else -> null
            }
            assertEquals(legacy, json.opt("output"))
            assertEquals(settings.dlnaOutput, json.getBoolean("dlnaOutput"))
            assertEquals(settings.miracastOutput, json.getBoolean("miracastOutput"))
            val imported = UserConfigCodec.decode(encoded, base)
            assertTrue(imported.errors.toString(), imported.errors.isEmpty())
            assertTrue(imported.ignoredKeys.isEmpty())
            assertEquals(settings, imported.settings)
            assertEquals(if (settings == base) 0 else 1, imported.changedCount)
            assertEquals(0, UserConfigCodec.decode(encoded, settings).changedCount)
        }
    }

    @Test fun explicitOutputsSkipEvenInvalidLegacyFieldsWithoutChangesOrIgnoredReports() {
        val json = JSONObject().put("outputs", org.json.JSONArray().put("dlna"))
            .put("output", "invalid").put("dlnaOutput", "invalid").put("miracastOutput", JSONObject.NULL)
        val imported = decode(json)
        assertTrue(imported.errors.toString(), imported.errors.isEmpty())
        assertTrue(imported.ignoredKeys.isEmpty())
        assertEquals(setOf(RelayOutput.DLNA), imported.settings.outputs)
        assertEquals(1, imported.changedCount)
        assertEquals(0, UserConfigCodec.decode(wrapped(json), imported.settings).changedCount)
    }

    @Test fun legacyOutputKeysApplyToCurrentMembersAndInvalidArraysRejectAtomically() {
        for (output in listOf("web", "carlink", "both")) for (dlna in listOf(false, true)) for (miracast in listOf(false, true)) {
            val json = JSONObject().put("output", output).put("dlnaOutput", dlna).put("miracastOutput", miracast)
            val imported = UserConfigCodec.decode(wrapped(json), base.copy(outputs = RelayOutput.entries.toSet()))
            assertTrue(imported.errors.toString(), imported.errors.isEmpty())
            assertEquals(OutputSelection.legacy(output != "web", output == "both", dlna, miracast), imported.settings.outputs)
        }
        val existing = base.copy(outputs = setOf(RelayOutput.CARLINK, RelayOutput.DLNA))
        assertEquals(setOf(RelayOutput.BROWSER, RelayOutput.DLNA),
            UserConfigCodec.decode(wrapped(JSONObject().put("output", "web")), existing).settings.outputs)
        assertEquals(mapOf("outputs" to "invalid"), UserConfigCodec.decode(wrapped(JSONObject().put("dlnaOutput", false)),
            base.copy(outputs = setOf(RelayOutput.DLNA))).errors)
        for (invalid in listOf<Any>(org.json.JSONArray(), org.json.JSONArray().put("web").put("web"),
            org.json.JSONArray().put("future"), org.json.JSONArray().put("web").put(1), "web", JSONObject.NULL)) {
            val imported = decode(JSONObject().put("outputs", invalid).put("autoStartOnBoot", true))
            assertEquals(mapOf("outputs" to "invalid"), imported.errors)
            assertEquals(base, imported.settings)
            assertEquals(0, imported.changedCount)
        }
    }

    @Test fun populatedRoundTripIncludesAllChoicesWithoutSecrets() {
        val settings = base.copy(outputs = setOf(RelayOutput.BROWSER, RelayOutput.CARLINK), connectionMode = ConnectionMode.WIRELESS, mfiTarget = MfiTarget.REMOTE,
            i2cPath = "/dev/i2c-1", remoteServer = "https://mfi.example.org", display = DisplayProfile(1920, 1080, 240),
            frameRate = FrameRatePreference.FPS_60, autoStartOnBoot = true, autoStartWhenReady = true, stage3d = true, highPerformance = false,
            localPreview = true, webPort = 8443, carlinkDeviceName = "Relay test", carlinkDeviceModel = "Test model",
            wirelessNetwork = WirelessNetworkSettings(WirelessNetworkChoice.PHONE_HOTSPOT, "Phone", "password1",
                ManualHotspotBand.GHZ_5, 36, ManualHotspotSecurity.WPA2, listOf(SharedWifiNetwork("Home", "password2"))),
            playCarPlayAudio = true, microphoneSource = MicrophoneSource.THIS_DEVICE, vehicleInformation = true,
            carlinkAutoReconnect = false, carlinkReconnectOnResize = true, displayMode = DisplayMode.FIXED,
            iphoneAddress = "12:34:56:78:9A:BC", plugins = true, pluginsEnabled = listOf("test.plugin"),
            carlinkTrace = true, carlinkHevc = ClinkHevcMode.SIGNAL, carlinkPictureSize = ClinkPictureSize.MODE,
            carplayResolution = CarPlayResolution.P720, carplayScreenWidth = CarPlayScreenWidth.MM300, viewAreas = false,
            car = CarSelection("tesla", model = "model-y", year = "model-y-2025"), videoCodec = VideoCodecSetting.H264,
            videoStats = VideoStatsWindow.SECONDS_15, carBluetoothAudio = false, teslaBrowser = TeslaBrowserMode.TESLA_ANDROID,
            teslaVirtualAddress = "169.254.88.2", teslaPublicAddress = "169.254.88.3",
            teslaMirrorAddress = "100.88.0.2", teslaAndroidAddress = "3.3.3.2")
        val developer = DeveloperOverrides(active = false, carlinkTrace = true, carlinkHevcEnabled = true,
            carlinkHevc = ClinkHevcMode.OFF, carlinkPictureSizeEnabled = true, carlinkPictureSize = ClinkPictureSize.MODE,
            physicalWidthEnabled = true, physicalWidthOverrideMm = 250, localPreview = true,
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
        assertEquals("tesla-android", json.getString("teslaBrowser"))
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
        assertTrue(imported.settings.browserOutput && imported.settings.carlinkOutput)
        assertEquals(WirelessNetworkChoice.EXISTING_WIFI, imported.settings.wirelessNetwork.choice)
        assertEquals(8443, imported.settings.webPort)
        assertEquals(listOf("wifiInterface"), imported.ignoredKeys)
        assertEquals(1, imported.settings.wirelessNetwork.sharedWifiNetworks.size)
    }
    @Test fun invalidValuesRejectEverythingAndUnknownValuesAreReported() {
        val result = decode(JSONObject().put("frameRate", "31").put("receiverPort", 80).put("autoStartOnBoot", true)
            .put("videoCodec", 1).put("unknown", false).put("hotspotChannel", 999)
            .put("developer", JSONObject().put("unknown", 0)))
        assertTrue(result.errors.keys.containsAll(listOf("frameRate", "receiverPort", "videoCodec")))
        assertTrue(result.settings == base)
        assertEquals(0, result.changedCount)
        assertEquals(listOf("developer.unknown", "hotspotChannel", "unknown"), result.ignoredKeys)
        for (value in listOf<Any>("false", 1, JSONObject.NULL)) assertTrue(decode(JSONObject().put("plugins", value)).errors.containsKey("plugins"))
    }
    @Test fun teslaModesUsePinnedWiresAndInvalidAddressesRejectWithTheirKey() {
        val modes = listOf(Triple(TeslaBrowserMode.VIRTUAL_NAME, "virtual-name", "teslaVirtualAddress"),
            Triple(TeslaBrowserMode.PUBLIC_ADDRESS, "public-address", "teslaPublicAddress"),
            Triple(TeslaBrowserMode.TESLAMIRROR, "teslamirror", "teslaMirrorAddress"),
            Triple(TeslaBrowserMode.TESLA_ANDROID, "tesla-android", "teslaAndroidAddress"))
        assertEquals("off", JSONObject(UserConfigCodec.encode(base, DeveloperOverrides(), meta)).getJSONObject("settings").getString("teslaBrowser"))
        for ((mode, wire, key) in modes) {
            val valid = decode(JSONObject().put("teslaBrowser", wire).put(key, "169.254.88.2"))
            assertTrue(valid.errors.toString(), valid.errors.isEmpty())
            assertEquals(mode, valid.settings.teslaBrowser)
            assertEquals(wire, JSONObject(UserConfigCodec.encode(valid.settings, valid.developer, meta)).getJSONObject("settings").getString("teslaBrowser"))
            val invalid = decode(JSONObject().put("teslaBrowser", wire).put(key, "192.168.1.1").put("frameRate", "60"))
            assertEquals(mapOf(key to "invalid"), invalid.errors)
            assertTrue(invalid.settings == base)
            assertEquals(0, invalid.changedCount)
            for (value in listOf<Any>(1, JSONObject.NULL)) assertTrue(decode(JSONObject().put(key, value)).errors.containsKey(key))
        }
        for (value in listOf<Any>("unknown", "TESLA_ANDROID", 1, JSONObject.NULL))
            assertEquals("invalid", decode(JSONObject().put("teslaBrowser", value)).errors["teslaBrowser"])
    }
    @Test fun missingTeslaKeysKeepCurrentValuesIncludingAnAssignedActiveAddress() {
        val current = base.copy(teslaBrowser = TeslaBrowserMode.PUBLIC_ADDRESS, teslaVirtualAddress = "169.254.88.2",
            teslaPublicAddress = "169.254.88.3", teslaMirrorAddress = "100.88.0.2", teslaAndroidAddress = "3.3.3.2")
        val result = UserConfigCodec.decode(wrapped(JSONObject().put("frameRate", "60")), current,
            phoneAddresses = setOf(current.teslaPublicAddress))
        assertTrue(result.errors.toString(), result.errors.isEmpty())
        assertTrue(result.settings == current.copy(frameRate = FrameRatePreference.FPS_60))
        assertEquals(1, result.changedCount)
    }
    @Test fun changedActiveTeslaAddressRejectsPhoneCollisions() {
        val address = "169.254.88.2"
        val text = wrapped(JSONObject().put("teslaBrowser", "public-address").put("teslaPublicAddress", address))
        val result = UserConfigCodec.decode(text.toByteArray(Charsets.UTF_8), base, phoneAddresses = setOf(address))
        assertEquals(mapOf("teslaPublicAddress" to "invalid"), result.errors)
        assertTrue(result.settings == base)
        assertEquals(0, result.changedCount)
        val current = base.copy(teslaBrowser = TeslaBrowserMode.PUBLIC_ADDRESS)
        assertEquals("invalid", UserConfigCodec.decode(text, current, phoneAddresses = setOf(address)).errors["teslaPublicAddress"])
        val remembered = base.copy(teslaPublicAddress = address)
        assertEquals("invalid", UserConfigCodec.decode(wrapped(JSONObject().put("teslaBrowser", "public-address")), remembered,
            phoneAddresses = setOf(address)).errors["teslaPublicAddress"])
        val unchanged = UserConfigCodec.decode(text, remembered.copy(teslaBrowser = TeslaBrowserMode.PUBLIC_ADDRESS), phoneAddresses = setOf(address))
        assertTrue(unchanged.errors.toString(), unchanged.errors.isEmpty())
        val inactive = UserConfigCodec.decode(wrapped(JSONObject().put("teslaPublicAddress", address)), base, phoneAddresses = setOf(address))
        assertTrue(inactive.errors.toString(), inactive.errors.isEmpty())
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
    @Test fun experimentalRoundTripPreservesEveryChoiceAndOldFilesImportDefaults() {
        val options = ExperimentalOptions(active = true, remotePlaybackProof = false, startupDiagnostics = false,
            screenSessionIsolation = false, screenReconnectReset = false, screenDeliverySerialization = false, rememberSubscription = true, keepScanning = true,
            recordAdvertisements = true, sendBluetoothAddress = true,
            isolateCarlinkNetwork = true, waitForCarlink = true, recoverWirelessNetwork = true, bluetoothAddress = "A0:B1:C2:D3:E4:F5")
        for (active in listOf(false, true)) {
            val chosen = base.copy(experimental = options.copy(active = active))
            val text = UserConfigCodec.encode(chosen, DeveloperOverrides(), meta)
            val json = JSONObject(text).getJSONObject("settings").getJSONObject("experimental")
            assertEquals(setOf("version", "active", "remotePlaybackProof", "cancelRecoveredRetries", "startupStatus", "startupDiagnostics", "networkObservationBackoff", "screenSessionIsolation", "screenReconnectReset", "screenDeliverySerialization",
                "iapChannelWhenOffered", "releaseBluetoothOnRequest", "awaitIphoneReconnect", "radioConflictPrompt", "stallWatchdog",
                "sendBluetoothAddress", "bluetoothAddress"),
                json.keys().asSequence().toSet())
            val result = UserConfigCodec.decode(text, base.copy(experimental = ExperimentalOptions.OFF))
            assertTrue(result.errors.toString(), result.errors.isEmpty())
            assertEquals(chosen, result.settings)
            assertTrue(result.ignoredKeys.isEmpty())
            assertTrue(result.changedCount > 0)
            assertEquals(0, UserConfigCodec.decode(text, chosen).changedCount)
            val legacy = UserConfigCodec.decode(wrapped(JSONObject()), chosen)
            assertTrue(legacy.errors.isEmpty())
            assertEquals(ExperimentalOptions(), legacy.settings.experimental)
        }
    }

    @Test fun laboratoryVersionMustBeTheSupportedInteger() {
        for (version in listOf<Any>(1, 3, "2", 2.5, true, JSONObject.NULL)) {
            val result = decode(JSONObject().put("experimental", JSONObject().put("version", version)))
            assertEquals(mapOf("experimental.version" to "invalid"), result.errors)
        }
    }

    @Test fun experimentalRejectsInvalidTypesAndActiveInvalidAddressesAtomically() {
        for (value in listOf<Any>(JSONObject.NULL, true, "on", org.json.JSONArray())) {
            val result = decode(JSONObject().put("experimental", value))
            assertEquals(mapOf("experimental" to "invalid"), result.errors)
        }
        for (key in listOf("remotePlaybackProof", "cancelRecoveredRetries", "startupStatus", "startupDiagnostics", "networkObservationBackoff", "screenSessionIsolation", "screenReconnectReset", "screenDeliverySerialization", "active", "rememberSubscription", "keepScanning", "recordAdvertisements", "sendBluetoothAddress", "bluetoothAddress",
            "isolateCarlinkNetwork", "waitForCarlink", "recoverWirelessNetwork", "iapChannelWhenOffered", "releaseBluetoothOnRequest", "awaitIphoneReconnect",
            "radioConflictPrompt", "stallWatchdog")) {
            val result = decode(JSONObject().put("experimental", JSONObject().put(key, 1)).put("autoStartOnBoot", true))
            assertEquals(mapOf("experimental.$key" to "invalid"), result.errors)
            assertEquals(base, result.settings)
        }
        val invalid = JSONObject().put("active", true).put("sendBluetoothAddress", true).put("bluetoothAddress", "02:00:00:00:00:00")
        val result = decode(JSONObject().put("experimental", invalid))
        assertEquals(mapOf("experimental.bluetoothAddress" to "invalid"), result.errors)
        assertEquals(base, result.settings)
        invalid.put("active", false).put("future", true)
        val parked = decode(JSONObject().put("experimental", invalid))
        assertTrue(parked.errors.isEmpty())
        assertEquals(listOf("experimental.future"), parked.ignoredKeys)
        assertFalse(parked.settings.experimental.sendBluetoothAddress)
        assertFalse(parked.settings.experimental.effective().sendBluetoothAddress)
    }


    @Test fun existingVersionTwoFilesEnableMissingScreenOptionsAndPreserveExplicitChoices() {
        val old = decode(JSONObject().put("experimental", JSONObject().put("version", 2).put("active", true)))
        assertTrue(old.errors.isEmpty())
        assertTrue(old.settings.experimental.screenSessionIsolation && old.settings.experimental.screenReconnectReset &&
            old.settings.experimental.screenDeliverySerialization)
        for (bits in 0..7) {
            val options = ExperimentalOptions(screenSessionIsolation = bits and 1 != 0,
                screenReconnectReset = bits and 2 != 0, screenDeliverySerialization = bits and 4 != 0)
            val chosen = base.copy(experimental = options)
            val result = UserConfigCodec.decode(UserConfigCodec.encode(chosen, DeveloperOverrides(), meta), base)
            assertTrue(result.errors.toString(), result.errors.isEmpty())
            assertEquals(options, result.settings.experimental)
        }
    }

    @Test fun handoffExperimentsRoundTripAndMissingKeysDefaultOnInsteadOfKeepingCurrentChoices() {
        for (active in listOf(false, true)) for (bits in 0..7) {
            val options = ExperimentalOptions(active = active, iapChannelWhenOffered = bits and 1 != 0,
                releaseBluetoothOnRequest = bits and 2 != 0, awaitIphoneReconnect = bits and 4 != 0)
            val chosen = base.copy(experimental = options)
            val text = UserConfigCodec.encode(chosen, DeveloperOverrides(), meta)
            val json = JSONObject(text)
            val experimental = json.getJSONObject("settings").getJSONObject("experimental")
            assertEquals(2, experimental.getInt("version"))
            assertEquals(options.iapChannelWhenOffered, experimental.getBoolean("iapChannelWhenOffered"))
            assertEquals(options.releaseBluetoothOnRequest, experimental.getBoolean("releaseBluetoothOnRequest"))
            assertEquals(options.awaitIphoneReconnect, experimental.getBoolean("awaitIphoneReconnect"))
            val result = UserConfigCodec.decode(text, base.copy(experimental = ExperimentalOptions.OFF))
            assertTrue(result.errors.toString(), result.errors.isEmpty())
            assertTrue(result.ignoredKeys.isEmpty())
            assertEquals(chosen, result.settings)
            experimental.remove("iapChannelWhenOffered"); experimental.remove("releaseBluetoothOnRequest")
            experimental.remove("awaitIphoneReconnect")
            val old = UserConfigCodec.decode(json.toString(), chosen.copy(experimental = ExperimentalOptions.OFF))
            assertTrue(old.errors.toString(), old.errors.isEmpty())
            assertTrue(old.ignoredKeys.isEmpty())
            assertEquals(options.copy(iapChannelWhenOffered = true, releaseBluetoothOnRequest = true, awaitIphoneReconnect = true),
                old.settings.experimental)
            assertEquals(active, old.settings.experimental.effective().iapChannelWhenOffered)
            assertEquals(active, old.settings.experimental.effective().releaseBluetoothOnRequest)
            assertEquals(active, old.settings.experimental.effective().awaitIphoneReconnect)
        }
    }

    @Test fun diagnosticsExperimentsRoundTripAndMissingKeysDefaultOn() {
        for (active in listOf(false, true)) for (bits in 0..3) {
            val options = ExperimentalOptions(active = active, radioConflictPrompt = bits and 1 != 0, stallWatchdog = bits and 2 != 0)
            val chosen = base.copy(experimental = options)
            val text = UserConfigCodec.encode(chosen, DeveloperOverrides(), meta)
            val json = JSONObject(text)
            val experimental = json.getJSONObject("settings").getJSONObject("experimental")
            assertEquals(options.radioConflictPrompt, experimental.getBoolean("radioConflictPrompt"))
            assertEquals(options.stallWatchdog, experimental.getBoolean("stallWatchdog"))
            val result = UserConfigCodec.decode(text, base.copy(experimental = ExperimentalOptions.OFF))
            assertTrue(result.errors.toString(), result.errors.isEmpty())
            assertEquals(chosen, result.settings)
            experimental.remove("radioConflictPrompt"); experimental.remove("stallWatchdog")
            val old = UserConfigCodec.decode(json.toString(), chosen.copy(experimental = ExperimentalOptions.OFF))
            assertTrue(old.errors.toString(), old.errors.isEmpty())
            assertEquals(options.copy(radioConflictPrompt = true, stallWatchdog = true), old.settings.experimental)
        }
    }
}
