// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-22
package org.openxplay.direlay.config

import org.json.JSONObject
import org.openxplay.direlay.settings.*
import com.shilapi.xcertplay.orchestration.*
import org.openxplay.stream.core.FrameRatePreference
import org.openxplay.stream.clink.ClinkHevcMode
import org.openxplay.stream.clink.ClinkPictureSize

class UserConfigImport(val settings: RelaySettings, val developer: DeveloperOverrides, val changedCount: Int,
    val ignoredKeys: List<String>, val errors: Map<String, String>) {
    override fun toString() = "UserConfigImport(changedCount=$changedCount, errors=${errors.size}, ignored=${ignoredKeys.size})"
}

/**
 * Version-1 key catalogue (enum strings are pinned in enumWires below):
 * - output: web/carlink/both; networkMode: device-hotspot/shared-wifi/wifi-direct.
 * - dlnaOutput, miracastOutput: booleans; dlnaRenderer: null or {udn: string, name: string};
 *   dlnaPicture: 720p/1080p.
 * - wirelessPicture: auto/custom; wirelessPictureSize: {width, height}, even pixels in
 *   landscape 640–1920 × 360–1080 (portrait bounds reversed); miracastStretch: boolean.
 * - sharedWifiNetworks: up to ten {ssid: 1–32 UTF-8 bytes, password: empty or 8–63 chars}.
 * - iphone: Bluetooth address or null; receiverPort: 1024–65535 or null.
 * - iphoneConnection, mfiTarget, hotspotBand, hotspotSecurity, displayMode, microphoneSource,
 *   carlinkHevc, carlinkPictureSize, carplayResolution, carplayScreenWidth, videoCodec, videoStats:
 *   the explicit enumWires values for their corresponding types.
 * - mfiI2cPath: NUL-free path (nonblank for i2c); mfiRemoteServer: NUL-free URL (HTTP(S) for remote).
 * - frameRate: auto/30/35/40/45/50/55/60; display: null or {width: even 320–3840, height: even 240–2160,
 *   density: 120–640}; fixed displayMode requires a profile.
 * - autoStartOnBoot, autoStartWhenReady, localPreview, playCarPlayAudio, vehicleInformation,
 *   carlinkAutoReconnect, carlinkReconnectOnResize, plugins, carlinkTrace, viewAreas,
 *   carBluetoothAudio: booleans; pluginsEnabled: unique valid plugin IDs.
 * - carlinkDeviceName, carlinkDeviceModel: nonblank 1–128 UTF-8 bytes without controls.
 * - car: {brand, brandText, model, modelText, year, yearText}, using CarSelection's normalized
 *   values. Year fields are optional on import; legacy leaf model IDs migrate to model + year.
 * - teslaBrowser: off/virtual-name/public-address/teslamirror/tesla-android; teslaVirtualAddress,
 *   teslaPublicAddress, teslaMirrorAddress, teslaAndroidAddress: strings; the active address must
 *   satisfy TeslaAddressPolicy and, when changed, must not collide with a phone IPv4 address.
 * - hotspotSsid: SSID; hotspotPassphrase: empty or 8–63 chars; hotspotChannel: 0 (automatic),
 *   1–14 or 32–177, compatible with hotspotBand. Other integer channels are ignored.
 * - developer: active, carlinkTrace, carlinkHevcEnabled, carlinkPictureSizeEnabled,
 *   physicalWidthEnabled, localPreview, playCarPlayAudio, microphoneSourceEnabled,
 *   webPortEnabled, carlinkDeviceModelEnabled are booleans; carlinkHevc, carlinkPictureSize,
 *   microphoneSource use enumWires; webPort uses the receiverPort range;
 *   physicalWidthOverrideMm is 0 or 100–400 in steps of 25; carlinkDeviceModel uses the name rule.
 * - Legacy fps is ignored. Without frameRate, developer.fpsEnabled=true and a valid
 *   developer.fps migrate to frameRate; otherwise both developer keys are ignored.
 * - experimental: active, rememberSubscription, keepScanning, recordAdvertisements,
 *   sendBluetoothAddress, isolateCarlinkNetwork, waitForCarlink and recoverWirelessNetwork are
 *   booleans; bluetoothAddress is a user-entered unicast Bluetooth address.
 * Linux aliases: output, networkMode, sharedWifiNetworks[].password, receiverPort and iphone.
 * Android aliases: iphoneConnection, mfiI2cPath, mfiRemoteServer and hotspot*.
 * All other keys match Kotlin properties; developer contains every remembered override.
 * remoteToken and all identity, trust, display-learning and history stores are excluded.
 */
object UserConfigCodec {
    const val FORMAT = "openxplay-user-config"
    private val enumWires: Map<Enum<*>, String> = mapOf(
        FrameRatePreference.AUTO to "auto", FrameRatePreference.FPS_30 to "30", FrameRatePreference.FPS_35 to "35",
        FrameRatePreference.FPS_40 to "40", FrameRatePreference.FPS_45 to "45", FrameRatePreference.FPS_50 to "50",
        FrameRatePreference.FPS_55 to "55", FrameRatePreference.FPS_60 to "60",
        ConnectionMode.USB to "usb", ConnectionMode.WIRELESS to "wireless", ConnectionMode.LAST_USED to "last-used",
        MfiTarget.LOCAL to "local", MfiTarget.USB_CH341 to "usb-ch341", MfiTarget.I2C to "i2c", MfiTarget.REMOTE to "remote",
        ManualHotspotBand.AUTO to "auto", ManualHotspotBand.GHZ_2_4 to "ghz-2-4", ManualHotspotBand.GHZ_5 to "ghz-5",
        ManualHotspotSecurity.OPEN to "open", ManualHotspotSecurity.WPA2 to "wpa2", ManualHotspotSecurity.WPA3_TRANSITION to "wpa3-transition", ManualHotspotSecurity.WPA3 to "wpa3",
        ClinkHevcMode.STREAM_ONLY to "stream-only", ClinkHevcMode.OFF to "off", ClinkHevcMode.SIGNAL to "signal",
        ClinkPictureSize.WINDOW to "window", ClinkPictureSize.MODE to "mode",
        DlnaPicture.P720 to "720p", DlnaPicture.P1080 to "1080p",
        WirelessPictureMode.AUTO to "auto", WirelessPictureMode.CUSTOM to "custom",
        DisplayMode.AUTOMATIC to "automatic", DisplayMode.RELAY_SCREEN to "relay-screen", DisplayMode.FIXED to "fixed",
        MicrophoneSource.HEAD_UNIT to "head-unit", MicrophoneSource.THIS_DEVICE to "this-device",
        CarPlayResolution.AUTO to "auto", CarPlayResolution.NATIVE to "native", CarPlayResolution.P1080 to "1080p", CarPlayResolution.P720 to "720p", CarPlayResolution.SCREEN to "screen",
        CarPlayScreenWidth.MM125 to "mm125", CarPlayScreenWidth.MM200 to "mm200", CarPlayScreenWidth.MM250 to "mm250", CarPlayScreenWidth.MM300 to "mm300",
        VideoCodecSetting.AUTO to "auto", VideoCodecSetting.H265 to "h265", VideoCodecSetting.H264 to "h264",
        VideoStatsWindow.OFF to "off", VideoStatsWindow.SECONDS_5 to "seconds-5", VideoStatsWindow.SECONDS_15 to "seconds-15",
        TeslaBrowserMode.OFF to "off", TeslaBrowserMode.VIRTUAL_NAME to "virtual-name", TeslaBrowserMode.PUBLIC_ADDRESS to "public-address",
        TeslaBrowserMode.TESLAMIRROR to "teslamirror", TeslaBrowserMode.TESLA_ANDROID to "tesla-android",
    )
    private fun wire(value: Enum<*>): String = enumWires.getValue(value)
    private inline fun <reified T : Enum<T>> enumValue(value: String): T =
        enumValues<T>().firstOrNull { wire(it) == value } ?: error("invalid")
    private fun networkWire(value: WirelessNetworkChoice) = when (value) {
        WirelessNetworkChoice.WIFI_DIRECT -> "wifi-direct"
        WirelessNetworkChoice.PHONE_HOTSPOT -> "device-hotspot"
        WirelessNetworkChoice.EXISTING_WIFI -> "shared-wifi"
    }
    fun encode(settings: RelaySettings, developer: DeveloperOverrides, meta: ConfigMeta): String =
        ConfigEnvelope.finish(ConfigEnvelope.create(FORMAT, meta).put("settings", values(settings, developer)))

    private fun values(s: RelaySettings, d: DeveloperOverrides): JSONObject = JSONObject()
        .put("output", if (s.bothOutputs) "both" else if (s.carlinkOutput) "carlink" else "web")
        .put("dlnaOutput", s.dlnaOutput)
        .put("dlnaRenderer", s.dlnaRenderer?.let { JSONObject().put("udn", it.udn).put("name", it.name) } ?: JSONObject.NULL)
        .put("dlnaPicture", wire(s.dlnaPicture))
        .put("miracastOutput", s.miracastOutput)
        .put("wirelessPicture", wire(s.wirelessPicture))
        .put("wirelessPictureSize", JSONObject().put("width", s.wirelessPictureSize.width).put("height", s.wirelessPictureSize.height))
        .put("miracastStretch", s.miracastStretch)
        .put("networkMode", networkWire(s.wirelessNetwork.choice))
        .put("sharedWifiNetworks", org.json.JSONArray().also { a -> s.wirelessNetwork.sharedWifiNetworks.forEach { a.put(JSONObject().put("ssid", it.ssid).put("password", it.passphrase)) } })
        .put("iphoneConnection", wire(s.connectionMode))
        .put("mfiTarget", wire(s.mfiTarget))
        .put("mfiI2cPath", s.i2cPath)
        .put("mfiRemoteServer", s.remoteServer)
        .put("frameRate", wire(s.frameRate))
        .put("receiverPort", s.webPort ?: JSONObject.NULL)
        .put("iphone", s.iphoneAddress ?: JSONObject.NULL)
        .put("autoStartOnBoot", s.autoStartOnBoot)
        .put("autoStartWhenReady", s.autoStartWhenReady)
        .put("localPreview", s.localPreview)
        .put("playCarPlayAudio", s.playCarPlayAudio)
        .put("vehicleInformation", s.vehicleInformation)
        .put("carlinkAutoReconnect", s.carlinkAutoReconnect)
        .put("carlinkReconnectOnResize", s.carlinkReconnectOnResize)
        .put("plugins", s.plugins)
        .put("carlinkTrace", s.carlinkTrace)
        .put("viewAreas", s.viewAreas)
        .put("carBluetoothAudio", s.carBluetoothAudio)
        .put("carlinkDeviceName", s.carlinkDeviceName)
        .put("carlinkDeviceModel", s.carlinkDeviceModel)
        .put("microphoneSource", wire(s.microphoneSource))
        .put("displayMode", wire(s.displayMode))
        .put("carlinkHevc", wire(s.carlinkHevc))
        .put("carlinkPictureSize", wire(s.carlinkPictureSize))
        .put("carplayResolution", wire(s.carplayResolution))
        .put("carplayScreenWidth", wire(s.carplayScreenWidth))
        .put("videoCodec", wire(s.videoCodec))
        .put("videoStats", wire(s.videoStats))
        .put("teslaBrowser", wire(s.teslaBrowser))
        .put("teslaVirtualAddress", s.teslaVirtualAddress)
        .put("teslaPublicAddress", s.teslaPublicAddress)
        .put("teslaMirrorAddress", s.teslaMirrorAddress)
        .put("teslaAndroidAddress", s.teslaAndroidAddress)
        .put("pluginsEnabled", org.json.JSONArray(s.pluginsEnabled))
        .put("display", s.display?.let { JSONObject().put("width", it.width).put("height", it.height).put("density", it.density) } ?: JSONObject.NULL)
        .put("car", carValues(s.car))
        .put("hotspotSsid", s.wirelessNetwork.manualSsid)
        .put("hotspotPassphrase", s.wirelessNetwork.manualPassphrase)
        .put("hotspotBand", wire(s.wirelessNetwork.band))
        .put("hotspotChannel", s.wirelessNetwork.channel)
        .put("hotspotSecurity", wire(s.wirelessNetwork.security))
        .put("developer", developerValues(d))
        .put("experimental", experimentalValues(s.experimental))

    private fun carValues(selection: CarSelection): JSONObject = selection.normalized().let { car ->
        JSONObject().put("brand", car.brand).put("brandText", car.brandText).put("model", car.model).put("modelText", car.modelText)
            .put("year", car.year).put("yearText", car.yearText)
    }

    private fun carValue(json: JSONObject): CarSelection {
        val car = CarSelection(json.string("brand"), json.string("brandText"), json.string("model"), json.string("modelText"),
            if (json.has("year")) json.string("year") else "", if (json.has("yearText")) json.string("yearText") else "")
        val normalized = car.normalized()
        val legacy = car.year.isEmpty() && car.model == normalized.year && normalized.model != car.model
        val expected = car.copy(model = if (legacy) normalized.model else car.model,
            year = if (legacy || car.year.isEmpty() && normalized.year == "other") normalized.year else car.year)
        require(expected == normalized)
        return normalized
    }

    private fun experimentalValues(e: ExperimentalOptions): JSONObject = JSONObject()
        .put("active", e.active)
        .put("rememberSubscription", e.rememberSubscription)
        .put("keepScanning", e.keepScanning)
        .put("recordAdvertisements", e.recordAdvertisements)
        .put("sendBluetoothAddress", e.sendBluetoothAddress)
        .put("bluetoothAddress", e.bluetoothAddress)
        .put("isolateCarlinkNetwork", e.isolateCarlinkNetwork)
        .put("waitForCarlink", e.waitForCarlink)
        .put("recoverWirelessNetwork", e.recoverWirelessNetwork)

    private fun developerValues(d: DeveloperOverrides): JSONObject = JSONObject()
        .put("active", d.active)
        .put("carlinkTrace", d.carlinkTrace)
        .put("carlinkHevcEnabled", d.carlinkHevcEnabled)
        .put("carlinkHevc", wire(d.carlinkHevc))
        .put("carlinkPictureSizeEnabled", d.carlinkPictureSizeEnabled)
        .put("carlinkPictureSize", wire(d.carlinkPictureSize))
        .put("physicalWidthEnabled", d.physicalWidthEnabled)
        .put("physicalWidthOverrideMm", d.physicalWidthOverrideMm)
        .put("localPreview", d.localPreview)
        .put("playCarPlayAudio", d.playCarPlayAudio)
        .put("microphoneSourceEnabled", d.microphoneSourceEnabled)
        .put("microphoneSource", wire(d.microphoneSource))
        .put("webPortEnabled", d.webPortEnabled)
        .put("webPort", d.webPort ?: JSONObject.NULL)
        .put("carlinkDeviceModelEnabled", d.carlinkDeviceModelEnabled)
        .put("carlinkDeviceModel", d.carlinkDeviceModel)

    fun decode(bytes: ByteArray, current: RelaySettings, choices: DeveloperOverrides = DeveloperOverrides(),
        wireless: Boolean = current.connectionMode == ConnectionMode.WIRELESS, phoneAddresses: Set<String> = emptySet()): UserConfigImport = try {
        decode(ConfigEnvelope.text(bytes), current, choices, wireless, phoneAddresses)
    } catch (e: ConfigFailure) { UserConfigImport(current, choices, 0, emptyList(), mapOf(e.key to e.code)) }

    fun decode(text: String, current: RelaySettings, choices: DeveloperOverrides = DeveloperOverrides(),
        wireless: Boolean = current.connectionMode == ConnectionMode.WIRELESS, phoneAddresses: Set<String> = emptySet()): UserConfigImport {
        var s = current.copy(experimental = ExperimentalOptions())
        var d = choices
        val errors = linkedMapOf<String, String>()
        val ignored = mutableListOf<String>()
        val root = try { ConfigEnvelope.read(text, FORMAT) }
        catch (e: ConfigFailure) { return UserConfigImport(current, choices, 0, ignored, mapOf(e.key to e.code)) }
        val j = root.optJSONObject("settings") ?: return UserConfigImport(current, choices, 0, ignored, mapOf("settings" to "invalid"))
        root.keys().asSequence().filter { it !in setOf("format", "version", "platform", "appVersion", "exportedAt", "settings") }
            .forEach { ignored.add(it) }
        fun unknown(objectValue: JSONObject, known: Set<String>, prefix: String) {
            objectValue.keys().asSequence().filter { it !in known }.forEach { ignored.add("$prefix.$it") }
        }
        j.optJSONObject("wirelessPictureSize")?.let { unknown(it, setOf("width", "height"), "wirelessPictureSize") }
        j.optJSONObject("dlnaRenderer")?.let { unknown(it, setOf("udn", "name"), "dlnaRenderer") }
        j.optJSONObject("display")?.let { unknown(it, setOf("width", "height", "density"), "display") }
        j.optJSONObject("car")?.let { unknown(it, setOf("brand", "brandText", "model", "modelText", "year", "yearText"), "car") }
        j.optJSONArray("sharedWifiNetworks")?.let { array -> for (index in 0 until array.length())
            array.optJSONObject(index)?.let { unknown(it, setOf("ssid", "password"), "sharedWifiNetworks[$index]") }
        }
        val before = values(current, choices)
        for (key in j.keys().asSequence().toList()) try {
            when (key) {
                "output" -> { val value = j.string(key).also { require(it in listOf("web", "carlink", "both")) }; s = s.copy(carlinkOutput = value != "web", bothOutputs = value == "both") }
                "dlnaOutput" -> { val value = j.bool(key); s = s.copy(dlnaOutput = value) }
                "dlnaRenderer" -> { val value = if (j.isNull(key)) null else j.getJSONObject(key).let { DlnaRendererChoice(it.string("udn"), it.string("name")) }; s = s.copy(dlnaRenderer = value) }
                "dlnaPicture" -> { val value = enumValue<DlnaPicture>(j.string(key)); s = s.copy(dlnaPicture = value) }
                "wirelessPicture" -> { val value = enumValue<WirelessPictureMode>(j.string(key)); s = s.copy(wirelessPicture = value) }
                "wirelessPictureSize" -> { val value = j.getJSONObject(key).let { WirelessPictureSize(it.integer("width"), it.integer("height")).also(WirelessPictureSize::validate) }; s = s.copy(wirelessPictureSize = value) }
                "miracastStretch" -> { val value = j.bool(key); s = s.copy(miracastStretch = value) }
                "miracastOutput" -> { val value = j.bool(key); s = s.copy(miracastOutput = value) }
                "networkMode" -> { val value = when (j.string(key)) { "device-hotspot" -> WirelessNetworkChoice.PHONE_HOTSPOT; "shared-wifi" -> WirelessNetworkChoice.EXISTING_WIFI; "wifi-direct" -> WirelessNetworkChoice.WIFI_DIRECT; else -> error("invalid") }; s = s.copy(wirelessNetwork = s.wirelessNetwork.copy(choice = value)) }
                "sharedWifiNetworks" -> { val value = j.getJSONArray(key).let { a -> (0 until a.length()).map { a.getJSONObject(it).let { o -> SharedWifiNetwork(o.string("ssid"), o.string("password")) } } }.also(SharedWifiNetworks::validate); s = s.copy(wirelessNetwork = s.wirelessNetwork.copy(sharedWifiNetworks = value)) }
                "iphoneConnection" -> { val value = enumValue<ConnectionMode>(j.string(key)); s = s.copy(connectionMode = value) }
                "mfiTarget" -> { val value = enumValue<MfiTarget>(j.string(key)); s = s.copy(mfiTarget = value) }
                "mfiI2cPath" -> { val value = j.string(key); s = s.copy(i2cPath = value) }
                "mfiRemoteServer" -> { val value = j.string(key); s = s.copy(remoteServer = value) }
                "frameRate" -> { val value = enumValue<FrameRatePreference>(j.string(key)); s = s.copy(frameRate = value) }
                "fps" -> ignored.add(key)
                "receiverPort" -> { val value = j.nullableInt(key); s = s.copy(webPort = value) }
                "iphone" -> { val value = j.nullableString(key); s = s.copy(iphoneAddress = value) }
                "autoStartOnBoot" -> { val value = j.bool(key); s = s.copy(autoStartOnBoot = value) }
                "autoStartWhenReady" -> { val value = j.bool(key); s = s.copy(autoStartWhenReady = value) }
                "localPreview" -> { val value = j.bool(key); s = s.copy(localPreview = value) }
                "playCarPlayAudio" -> { val value = j.bool(key); s = s.copy(playCarPlayAudio = value) }
                "vehicleInformation" -> { val value = j.bool(key); s = s.copy(vehicleInformation = value) }
                "carlinkAutoReconnect" -> { val value = j.bool(key); s = s.copy(carlinkAutoReconnect = value) }
                "carlinkReconnectOnResize" -> { val value = j.bool(key); s = s.copy(carlinkReconnectOnResize = value) }
                "plugins" -> { val value = j.bool(key); s = s.copy(plugins = value) }
                "carlinkTrace" -> { val value = j.bool(key); s = s.copy(carlinkTrace = value) }
                "viewAreas" -> { val value = j.bool(key); s = s.copy(viewAreas = value) }
                "carBluetoothAudio" -> { val value = j.bool(key); s = s.copy(carBluetoothAudio = value) }
                "carlinkDeviceName" -> { val value = j.string(key); s = s.copy(carlinkDeviceName = value) }
                "carlinkDeviceModel" -> { val value = j.string(key); s = s.copy(carlinkDeviceModel = value) }
                "microphoneSource" -> { val value = enumValue<MicrophoneSource>(j.string(key)); s = s.copy(microphoneSource = value) }
                "displayMode" -> { val value = enumValue<DisplayMode>(j.string(key)); s = s.copy(displayMode = value) }
                "carlinkHevc" -> { val value = enumValue<ClinkHevcMode>(j.string(key)); s = s.copy(carlinkHevc = value) }
                "carlinkPictureSize" -> { val value = enumValue<ClinkPictureSize>(j.string(key)); s = s.copy(carlinkPictureSize = value) }
                "carplayResolution" -> { val value = enumValue<CarPlayResolution>(j.string(key)); s = s.copy(carplayResolution = value) }
                "carplayScreenWidth" -> { val value = enumValue<CarPlayScreenWidth>(j.string(key)); s = s.copy(carplayScreenWidth = value) }
                "videoCodec" -> { val value = enumValue<VideoCodecSetting>(j.string(key)); s = s.copy(videoCodec = value) }
                "videoStats" -> { val value = enumValue<VideoStatsWindow>(j.string(key)); s = s.copy(videoStats = value) }
                "teslaBrowser" -> { val value = enumValue<TeslaBrowserMode>(j.string(key)); s = s.copy(teslaBrowser = value) }
                "teslaVirtualAddress" -> { val value = j.string(key); s = s.copy(teslaVirtualAddress = value) }
                "teslaPublicAddress" -> { val value = j.string(key); s = s.copy(teslaPublicAddress = value) }
                "teslaMirrorAddress" -> { val value = j.string(key); s = s.copy(teslaMirrorAddress = value) }
                "teslaAndroidAddress" -> { val value = j.string(key); s = s.copy(teslaAndroidAddress = value) }
                "pluginsEnabled" -> { val value = j.getJSONArray(key).let { a -> (0 until a.length()).map { require(a.get(it) is String); a.getString(it) } }; s = s.copy(pluginsEnabled = value) }
                "display" -> { val value = if (j.isNull(key)) null else j.getJSONObject(key).let { DisplayProfile(it.integer("width"), it.integer("height"), it.integer("density")).also(DisplayProfile::validate) }; s = s.copy(display = value) }
                "car" -> { val value = carValue(j.getJSONObject(key)); s = s.copy(car = value) }
                "hotspotSsid" -> { val value = j.string(key); s = s.copy(wirelessNetwork = s.wirelessNetwork.copy(manualSsid = value)) }
                "hotspotPassphrase" -> { val value = j.string(key); s = s.copy(wirelessNetwork = s.wirelessNetwork.copy(manualPassphrase = value)) }
                "hotspotBand" -> { val value = enumValue<ManualHotspotBand>(j.string(key)); s = s.copy(wirelessNetwork = s.wirelessNetwork.copy(band = value)) }
                "hotspotChannel" -> {
                    val value = j.integer(key)
                    if (value != 0 && value !in 1..14 && value !in 32..177) ignored.add(key)
                    else s = s.copy(wirelessNetwork = s.wirelessNetwork.copy(channel = value))
                }
                "hotspotSecurity" -> { val value = enumValue<ManualHotspotSecurity>(j.string(key)); s = s.copy(wirelessNetwork = s.wirelessNetwork.copy(security = value)) }
                "experimental" -> {
                    val objectValue = j.getJSONObject(key)
                    var e = ExperimentalOptions()
                    for (field in objectValue.keys().asSequence().toList()) try {
                        e = when (field) {
                            "active" -> e.copy(active = objectValue.bool(field))
                            "rememberSubscription" -> e.copy(rememberSubscription = objectValue.bool(field))
                            "keepScanning" -> e.copy(keepScanning = objectValue.bool(field))
                            "recordAdvertisements" -> e.copy(recordAdvertisements = objectValue.bool(field))
                            "sendBluetoothAddress" -> e.copy(sendBluetoothAddress = objectValue.bool(field))
                            "bluetoothAddress" -> e.copy(bluetoothAddress = objectValue.string(field))
                            "isolateCarlinkNetwork" -> e.copy(isolateCarlinkNetwork = objectValue.bool(field))
                            "waitForCarlink" -> e.copy(waitForCarlink = objectValue.bool(field))
                            "recoverWirelessNetwork" -> e.copy(recoverWirelessNetwork = objectValue.bool(field))
                            else -> { ignored.add("experimental.$field"); e }
                        }
                    } catch (_: Exception) { errors.putIfAbsent("experimental.$field", "invalid") }
                    s = s.copy(experimental = e)
                }
                "developer" -> {
                    val j = j.getJSONObject(key)
                    for (key in j.keys().asSequence().toList()) try {
                        when (key) {
                            "active" -> { val value = j.bool(key); d = d.copy(active = value) }
                            "carlinkTrace" -> { val value = j.bool(key); d = d.copy(carlinkTrace = value) }
                            "carlinkHevcEnabled" -> { val value = j.bool(key); d = d.copy(carlinkHevcEnabled = value) }
                            "carlinkHevc" -> { val value = enumValue<ClinkHevcMode>(j.string(key)); d = d.copy(carlinkHevc = value) }
                            "carlinkPictureSizeEnabled" -> { val value = j.bool(key); d = d.copy(carlinkPictureSizeEnabled = value) }
                            "carlinkPictureSize" -> { val value = enumValue<ClinkPictureSize>(j.string(key)); d = d.copy(carlinkPictureSize = value) }
                            "physicalWidthEnabled" -> { val value = j.bool(key); d = d.copy(physicalWidthEnabled = value) }
                            "physicalWidthOverrideMm" -> { val value = j.integer(key); d = d.copy(physicalWidthOverrideMm = value) }
                            "fpsEnabled", "fps" -> Unit
                            "localPreview" -> { val value = j.bool(key); d = d.copy(localPreview = value) }
                            "playCarPlayAudio" -> { val value = j.bool(key); d = d.copy(playCarPlayAudio = value) }
                            "microphoneSourceEnabled" -> { val value = j.bool(key); d = d.copy(microphoneSourceEnabled = value) }
                            "microphoneSource" -> { val value = enumValue<MicrophoneSource>(j.string(key)); d = d.copy(microphoneSource = value) }
                            "webPortEnabled" -> { val value = j.bool(key); d = d.copy(webPortEnabled = value) }
                            "webPort" -> { val value = j.nullableInt(key); d = d.copy(webPort = value) }
                            "carlinkDeviceModelEnabled" -> { val value = j.bool(key); d = d.copy(carlinkDeviceModelEnabled = value) }
                            "carlinkDeviceModel" -> { val value = j.string(key); d = d.copy(carlinkDeviceModel = value) }
                            else -> ignored.add("developer.$key")
                        }
                    } catch (_: Exception) { errors.putIfAbsent("developer.$key", "invalid") }
                }
                else -> ignored.add(key)
            }
        } catch (_: Exception) { errors.putIfAbsent(key, "invalid") }
        j.optJSONObject("developer")?.let { legacy ->
            val migrated = if (!j.has("frameRate") && legacy.opt("fpsEnabled") == true) {
                runCatching { legacy.integer("fps") }.getOrNull()?.let { fps ->
                    FrameRatePreference.entries.firstOrNull { it.name == "FPS_$fps" }
                }
            } else null
            if (migrated != null) s = s.copy(frameRate = migrated)
            else listOf("fpsEnabled", "fps").filter(legacy::has).forEach { ignored.add("developer.$it") }
        }
        fun check(key: String, test: () -> Unit) { try { test() } catch (_: Exception) { errors.putIfAbsent(key, "invalid") } }
        check("experimental.bluetoothAddress") { require(!s.experimental.invalidAddress) }
        val teslaKey = when (s.teslaBrowser) {
            TeslaBrowserMode.OFF -> null
            TeslaBrowserMode.VIRTUAL_NAME -> "teslaVirtualAddress"
            TeslaBrowserMode.PUBLIC_ADDRESS -> "teslaPublicAddress"
            TeslaBrowserMode.TESLAMIRROR -> "teslaMirrorAddress"
            TeslaBrowserMode.TESLA_ANDROID -> "teslaAndroidAddress"
        }
        if (teslaKey != null) check(teslaKey) {
            val address = checkNotNull(s.teslaAddress())
            require(TeslaAddressPolicy.valid(address, if (address != current.teslaAddress()) phoneAddresses else emptySet()))
        }
        check("receiverPort") { require(s.webPort?.let { it in 1024..65535 } != false) }
        check("iphone") { require(s.iphoneAddress == null || Regex("[0-9A-Fa-f]{2}(:[0-9A-Fa-f]{2}){5}").matches(s.iphoneAddress!!)) }
        check("pluginsEnabled") { require(org.openxplay.direlay.plugins.RelayPlugins.validIds(s.pluginsEnabled)) }
        for ((key, value) in listOf("carlinkDeviceName" to s.carlinkDeviceName, "carlinkDeviceModel" to s.carlinkDeviceModel))
            check(key) { require(value.isNotBlank() && value.toByteArray(Charsets.UTF_8).size <= 128 && value.none(Character::isISOControl)) }
        check("display") { s.display?.validate(); require(s.displayMode != DisplayMode.FIXED || s.display != null) }
        check("sharedWifiNetworks") { SharedWifiNetworks.validate(s.wirelessNetwork.sharedWifiNetworks) }
        val useWireless = s.connectionMode.wireless(wireless)
        if (useWireless && s.wirelessNetwork.choice == WirelessNetworkChoice.PHONE_HOTSPOT) {
            val network = s.wirelessNetwork
            check("hotspotSsid") {
                require(network.manualSsid.isEmpty() && CarlinkConnectionPolicy.usesWifiDirect(s) ||
                    network.manualSsid.isNotBlank() && network.manualSsid.toByteArray(Charsets.UTF_8).size in 1..32 && '\u0000' !in network.manualSsid)
            }
            check("hotspotPassphrase") { require((network.manualPassphrase.isEmpty() || network.manualPassphrase.length in 8..63) && '\u0000' !in network.manualPassphrase) }
            check("hotspotChannel") { require(network.channel == 0 || isManualHotspotChannelCompatible(network.band, network.channel)) }
            check("hotspotSecurity") { require(network.security == ManualHotspotSecurity.OPEN || network.manualPassphrase.length in 8..63) }
        }
        if (useWireless && errors.isEmpty()) check("networkMode") { s.wirelessNetwork.validate(true, CarlinkConnectionPolicy.usesWifiDirect(s)) }
        check("mfiI2cPath") { require('\u0000' !in s.i2cPath && (s.mfiTarget != MfiTarget.I2C || s.i2cPath.isNotBlank())) }
        check("mfiRemoteServer") {
            require('\u0000' !in s.remoteServer)
            if (s.mfiTarget == MfiTarget.REMOTE) java.net.URI(s.remoteServer.trim()).also { require(it.scheme in listOf("http", "https") && !it.host.isNullOrBlank()) }
        }
        check("developer.webPort") { require(d.webPort?.let { it in 1024..65535 } != false) }
        check("developer.physicalWidthOverrideMm") { require(d.physicalWidthOverrideMm == 0 || d.physicalWidthOverrideMm in 100..400 && d.physicalWidthOverrideMm % 25 == 0) }
        check("developer.carlinkDeviceModel") { require(d.carlinkDeviceModel.isNotBlank() && d.carlinkDeviceModel.toByteArray(Charsets.UTF_8).size <= 128 && d.carlinkDeviceModel.none(Character::isISOControl)) }
        // Run the same full validation and incomplete-network allowance as the editor.
        if (errors.isEmpty()) check("settings") { s.validate(useWireless, allowEmptySharedWifi = true); d.effectiveSettings(s).validate(useWireless, allowEmptySharedWifi = true) }
        val after = values(s, d)
        var changed = before.keys().asSequence().filter { it !in setOf("developer", "experimental") }.count { before.get(it).toString() != after.get(it).toString() }
        val oldDev = before.getJSONObject("developer")
        val newDev = after.getJSONObject("developer")
        changed += oldDev.keys().asSequence().count { oldDev.get(it).toString() != newDev.get(it).toString() }
        val oldExperimental = before.getJSONObject("experimental")
        val newExperimental = after.getJSONObject("experimental")
        changed += oldExperimental.keys().asSequence().count { oldExperimental.get(it).toString() != newExperimental.get(it).toString() }
        return if (errors.isEmpty()) UserConfigImport(s, d, changed, ignored.sorted(), errors)
        else UserConfigImport(current, choices, 0, ignored.sorted(), errors)
    }
}
