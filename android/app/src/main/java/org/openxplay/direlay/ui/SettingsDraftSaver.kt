// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-14
package org.openxplay.direlay.ui

import android.os.Bundle
import androidx.compose.runtime.saveable.Saver
import com.shilapi.xcertplay.orchestration.ManualHotspotBand
import com.shilapi.xcertplay.orchestration.ManualHotspotSecurity
import com.shilapi.xcertplay.orchestration.MfiTarget
import org.openxplay.direlay.settings.*
import org.openxplay.stream.core.FrameRatePreference
import org.openxplay.stream.clink.ClinkHevcMode
import org.openxplay.stream.clink.ClinkPictureSize

/** Remember both the switches and parked values across rotation, including inactive overrides. */
fun developerOverridesSaver(base: DeveloperOverrides) = Saver<DeveloperOverrides, Bundle>(save = { value ->
    Bundle().apply {
        putBoolean("active", value.active)
        putBoolean("carlinkTrace", value.carlinkTrace)
        putBoolean("carlinkHevcEnabled", value.carlinkHevcEnabled)
        putString("carlinkHevc", value.carlinkHevc.name)
        putBoolean("carlinkPictureSizeEnabled", value.carlinkPictureSizeEnabled)
        putString("carlinkPictureSize", value.carlinkPictureSize.name)
        putBoolean("physicalWidthEnabled", value.physicalWidthEnabled)
        putInt("physicalWidthOverrideMm", value.physicalWidthOverrideMm)
        putBoolean("localPreview", value.localPreview)
        putBoolean("playCarPlayAudio", value.playCarPlayAudio)
        putBoolean("microphoneSourceEnabled", value.microphoneSourceEnabled)
        putString("microphoneSource", value.microphoneSource.name)
        putBoolean("webPortEnabled", value.webPortEnabled)
        putInt("webPort", value.webPort ?: 0)
        putBoolean("carlinkDeviceModelEnabled", value.carlinkDeviceModelEnabled)
        putString("carlinkDeviceModel", value.carlinkDeviceModel)
    }
}, restore = { bundle ->
    base.copy(
        active = bundle.getBoolean("active", base.active),
        carlinkTrace = bundle.getBoolean("carlinkTrace", base.carlinkTrace),
        carlinkHevcEnabled = bundle.getBoolean("carlinkHevcEnabled", base.carlinkHevcEnabled),
        carlinkHevc = ClinkHevcMode.entries.firstOrNull { it.name == bundle.getString("carlinkHevc") } ?: base.carlinkHevc,
        carlinkPictureSizeEnabled = bundle.getBoolean("carlinkPictureSizeEnabled", base.carlinkPictureSizeEnabled),
        carlinkPictureSize = ClinkPictureSize.entries.firstOrNull { it.name == bundle.getString("carlinkPictureSize") } ?: base.carlinkPictureSize,
        physicalWidthEnabled = bundle.getBoolean("physicalWidthEnabled", base.physicalWidthEnabled),
        physicalWidthOverrideMm = bundle.getInt("physicalWidthOverrideMm", base.physicalWidthOverrideMm),
        localPreview = bundle.getBoolean("localPreview", base.localPreview),
        playCarPlayAudio = bundle.getBoolean("playCarPlayAudio", base.playCarPlayAudio),
        microphoneSourceEnabled = bundle.getBoolean("microphoneSourceEnabled", base.microphoneSourceEnabled),
        microphoneSource = MicrophoneSource.entries.firstOrNull { it.name == bundle.getString("microphoneSource") } ?: base.microphoneSource,
        webPortEnabled = bundle.getBoolean("webPortEnabled", base.webPortEnabled),
        webPort = bundle.getInt("webPort", base.webPort ?: 0).takeIf { it != 0 },
        carlinkDeviceModelEnabled = bundle.getBoolean("carlinkDeviceModelEnabled", base.carlinkDeviceModelEnabled),
        carlinkDeviceModel = bundle.getString("carlinkDeviceModel") ?: base.carlinkDeviceModel,
    )
})

/** Rotation retains unsaved choices without publishing them into the runtime's preferences. */
fun relaySettingsSaver(base: RelaySettings) = Saver<RelaySettings, Bundle>(save = { value ->
    Bundle().apply {
        putStringArrayList("text", arrayListOf(value.connectionMode.name, value.mfiTarget.name,
            value.i2cPath, value.remoteServer, value.remoteToken, value.carlinkDeviceName, value.carlinkDeviceModel,
            value.microphoneSource.name, value.wirelessNetwork.choice.name, value.wirelessNetwork.manualSsid,
            value.wirelessNetwork.manualPassphrase, value.wirelessNetwork.band.name, value.wirelessNetwork.security.name,
            SharedWifiNetworks.encode(value.wirelessNetwork.sharedWifiNetworks)))
        // Slots 3 and 5 stay reserved so older bundles keep the other flag indices.
        putBooleanArray("flags", booleanArrayOf(value.autoStartOnBoot, value.autoStartWhenReady,
            value.localPreview, false, value.playCarPlayAudio, false, value.vehicleInformation))
        putString("frameRate", value.frameRate.name)
        putIntArray("numbers", intArrayOf(0, value.webPort ?: 0, value.wirelessNetwork.channel))
        putStringArray("outputs", RelayOutput.entries.filter { it in value.outputs }.map { it.wire }.toTypedArray())
        putString("dlnaRendererUdn", value.dlnaRenderer?.udn)
        putString("dlnaRendererName", value.dlnaRenderer?.name)
        putString("dlnaPicture", value.dlnaPicture.name)
        putString("wirelessPicture", value.wirelessPicture.name)
        putInt("wirelessPictureWidth", value.wirelessPictureSize.width)
        putInt("wirelessPictureHeight", value.wirelessPictureSize.height)
        putString("carlinkPictureSizes", CarlinkPictureSizes.savedJson(value.carlinkPictureSizes).toString())
        putString("carlinkPictureDefault", value.carlinkPictureDefault?.let { CarlinkPictureSizes.json(it).toString() })
        putBoolean("miracastStretch", value.miracastStretch)
        putBoolean("experimental.active", value.experimental.active)
        putBoolean("experimental.remotePlaybackProof", value.experimental.remotePlaybackProof)
        putBoolean("experimental.cancelRecoveredRetries", value.experimental.cancelRecoveredRetries)
        putBoolean("experimental.startupStatus", value.experimental.startupStatus)
        putBoolean("experimental.startupDiagnostics", value.experimental.startupDiagnostics)
        putBoolean("experimental.networkObservationBackoff", value.experimental.networkObservationBackoff)
        putBoolean("experimental.screenSessionIsolation", value.experimental.screenSessionIsolation)
        putBoolean("experimental.screenReconnectReset", value.experimental.screenReconnectReset)
        putBoolean("experimental.screenDeliverySerialization", value.experimental.screenDeliverySerialization)
        putBoolean("experimental.iapChannelWhenOffered", value.experimental.iapChannelWhenOffered)
        putBoolean("experimental.releaseBluetoothOnRequest", value.experimental.releaseBluetoothOnRequest)
        putBoolean("experimental.awaitIphoneReconnect", value.experimental.awaitIphoneReconnect)
        putBoolean("experimental.radioConflictPrompt", value.experimental.radioConflictPrompt)
        putBoolean("experimental.stallWatchdog", value.experimental.stallWatchdog)
        putBoolean("experimental.rememberSubscription", value.experimental.rememberSubscription)
        putBoolean("experimental.keepScanning", value.experimental.keepScanning)
        putBoolean("experimental.recordAdvertisements", value.experimental.recordAdvertisements)
        putBoolean("experimental.sendBluetoothAddress", value.experimental.sendBluetoothAddress)
        putString("experimental.bluetoothAddress", value.experimental.bluetoothAddress)
        putBoolean("experimental.isolateCarlinkNetwork", value.experimental.isolateCarlinkNetwork)
        putBoolean("experimental.waitForCarlink", value.experimental.waitForCarlink)
        putBoolean("experimental.recoverWirelessNetwork", value.experimental.recoverWirelessNetwork)
        putBoolean("carlinkAutoReconnect", value.carlinkAutoReconnect)
        putBoolean("carlinkReconnectOnResize", value.carlinkReconnectOnResize)
        putBoolean("carlinkTrace", value.carlinkTrace)
        putString("carlinkHevc", value.carlinkHevc.name)
        putString("carlinkPictureSize", value.carlinkPictureSize.name)
        putBoolean("carPlayViewAreas", value.viewAreas)
        putBoolean("carBluetoothAudio", value.carBluetoothAudio)
        putStringArrayList("car", arrayListOf(value.car.brand, value.car.brandText, value.car.model, value.car.modelText, value.car.year, value.car.yearText))
        putString("iphoneAddress", value.iphoneAddress)
        putString("displayMode", value.displayMode.name)
        putString("videoCodec", value.videoCodec.name)
        putString("videoStats", value.videoStats.name)
        putString("teslaBrowser", value.teslaBrowser.name)
        putString("teslaVirtualAddress", value.teslaVirtualAddress)
        putString("teslaPublicAddress", value.teslaPublicAddress)
        putString("teslaMirrorAddress", value.teslaMirrorAddress)
        putString("teslaAndroidAddress", value.teslaAndroidAddress)
        putString("carplayResolution", value.carplayResolution.name)
        putInt("carplayScreenWidth", value.carplayScreenWidth.mm)
        value.display?.let { putIntArray("display", intArrayOf(it.width, it.height, it.density)) }
    }
}, restore = { bundle ->
    val text = requireNotNull(bundle.getStringArrayList("text"))
    val flags = requireNotNull(bundle.getBooleanArray("flags"))
    val numbers = requireNotNull(bundle.getIntArray("numbers"))
    base.copy(experimental = ExperimentalOptions(
        active = bundle.getBoolean("experimental.active", base.experimental.active),
        remotePlaybackProof = bundle.getBoolean("experimental.remotePlaybackProof", base.experimental.remotePlaybackProof),
        cancelRecoveredRetries = bundle.getBoolean("experimental.cancelRecoveredRetries", base.experimental.cancelRecoveredRetries),
        startupStatus = bundle.getBoolean("experimental.startupStatus", base.experimental.startupStatus),
        startupDiagnostics = bundle.getBoolean("experimental.startupDiagnostics", base.experimental.startupDiagnostics),
        networkObservationBackoff = bundle.getBoolean("experimental.networkObservationBackoff", base.experimental.networkObservationBackoff),
        screenSessionIsolation = bundle.getBoolean("experimental.screenSessionIsolation", base.experimental.screenSessionIsolation),
        screenReconnectReset = bundle.getBoolean("experimental.screenReconnectReset", base.experimental.screenReconnectReset),
        screenDeliverySerialization = bundle.getBoolean("experimental.screenDeliverySerialization", base.experimental.screenDeliverySerialization),
        iapChannelWhenOffered = bundle.getBoolean("experimental.iapChannelWhenOffered", base.experimental.iapChannelWhenOffered),
        releaseBluetoothOnRequest = bundle.getBoolean("experimental.releaseBluetoothOnRequest", base.experimental.releaseBluetoothOnRequest),
        awaitIphoneReconnect = bundle.getBoolean("experimental.awaitIphoneReconnect", base.experimental.awaitIphoneReconnect),
        radioConflictPrompt = bundle.getBoolean("experimental.radioConflictPrompt", base.experimental.radioConflictPrompt),
        stallWatchdog = bundle.getBoolean("experimental.stallWatchdog", base.experimental.stallWatchdog),
        rememberSubscription = bundle.getBoolean("experimental.rememberSubscription", base.experimental.rememberSubscription),
        keepScanning = bundle.getBoolean("experimental.keepScanning", base.experimental.keepScanning),
        recordAdvertisements = bundle.getBoolean("experimental.recordAdvertisements", base.experimental.recordAdvertisements),
        sendBluetoothAddress = bundle.getBoolean("experimental.sendBluetoothAddress", base.experimental.sendBluetoothAddress),
        bluetoothAddress = bundle.getString("experimental.bluetoothAddress") ?: base.experimental.bluetoothAddress,
        isolateCarlinkNetwork = bundle.getBoolean("experimental.isolateCarlinkNetwork", base.experimental.isolateCarlinkNetwork),
        waitForCarlink = bundle.getBoolean("experimental.waitForCarlink", base.experimental.waitForCarlink),
        recoverWirelessNetwork = bundle.getBoolean("experimental.recoverWirelessNetwork", base.experimental.recoverWirelessNetwork)),
        connectionMode = ConnectionMode.valueOf(text[0]), mfiTarget = MfiTarget.valueOf(text[1]),
        i2cPath = text[2], remoteServer = text[3], remoteToken = text[4], carlinkDeviceName = text[5], carlinkDeviceModel = text[6],
        microphoneSource = MicrophoneSource.valueOf(text[7]),
        autoStartOnBoot = flags[0], autoStartWhenReady = flags[1], localPreview = flags[2],
        playCarPlayAudio = flags[4], vehicleInformation = flags.getOrElse(6) { false }, carlinkAutoReconnect = bundle.getBoolean("carlinkAutoReconnect", flags.getOrElse(8) { true }), webPort = numbers[1].takeIf { it != 0 },
        frameRate = FrameRatePreference.entries.firstOrNull { it.name == bundle.getString("frameRate") } ?: base.frameRate,
        videoStats = if (bundle.containsKey("videoStats")) VideoStatsWindow.fromStored(bundle.getString("videoStats")) else base.videoStats,
        teslaBrowser = TeslaBrowserMode.entries.firstOrNull { it.name == bundle.getString("teslaBrowser") } ?: base.teslaBrowser,
        teslaVirtualAddress = bundle.getString("teslaVirtualAddress") ?: base.teslaVirtualAddress,
        teslaPublicAddress = bundle.getString("teslaPublicAddress") ?: base.teslaPublicAddress,
        teslaMirrorAddress = bundle.getString("teslaMirrorAddress") ?: base.teslaMirrorAddress,
        teslaAndroidAddress = bundle.getString("teslaAndroidAddress") ?: base.teslaAndroidAddress,
        outputs = bundle.getStringArray("outputs")?.takeIf { wires -> wires.isNotEmpty() &&
            wires.all { wire -> RelayOutput.entries.any { it.wire == wire } } }
            ?.map { wire -> RelayOutput.entries.first { it.wire == wire } }?.toSet() ?: base.outputs,
        dlnaRenderer = if (bundle.containsKey("dlnaRendererUdn")) bundle.getString("dlnaRendererUdn")?.let {
            DlnaRendererChoice(it, bundle.getString("dlnaRendererName").orEmpty()) } else base.dlnaRenderer,
        dlnaPicture = DlnaPicture.entries.firstOrNull { it.name == bundle.getString("dlnaPicture") } ?: base.dlnaPicture,
        wirelessPicture = WirelessPictureMode.entries.firstOrNull { it.name == bundle.getString("wirelessPicture") } ?: base.wirelessPicture,
        wirelessPictureSize = WirelessPictureSize(bundle.getInt("wirelessPictureWidth", base.wirelessPictureSize.width),
            bundle.getInt("wirelessPictureHeight", base.wirelessPictureSize.height)),
        carlinkPictureSizes = if (bundle.containsKey("carlinkPictureSizes"))
            CarlinkPictureSizes.decodeSaved(bundle.getString("carlinkPictureSizes").orEmpty()) else base.carlinkPictureSizes,
        carlinkPictureDefault = if (bundle.containsKey("carlinkPictureDefault"))
            CarlinkPictureSizes.decodeDefault(bundle.getString("carlinkPictureDefault")) else base.carlinkPictureDefault,
        miracastStretch = bundle.getBoolean("miracastStretch", base.miracastStretch),
        carlinkReconnectOnResize = bundle.getBoolean("carlinkReconnectOnResize", base.carlinkReconnectOnResize),
        videoCodec = VideoCodecSetting.entries.firstOrNull { it.name == bundle.getString("videoCodec") } ?: base.videoCodec,
        carplayResolution = CarPlayResolution.entries.firstOrNull { it.name == bundle.getString("carplayResolution") } ?: base.carplayResolution,
        carplayScreenWidth = CarPlayScreenWidth.entries.firstOrNull { it.mm == bundle.getInt("carplayScreenWidth", base.carplayScreenWidth.mm) } ?: base.carplayScreenWidth,
        carlinkTrace = bundle.getBoolean("carlinkTrace", false),
        carlinkHevc = org.openxplay.stream.clink.ClinkHevcMode.entries.firstOrNull { it.name == bundle.getString("carlinkHevc") } ?: base.carlinkHevc,
        carlinkPictureSize = org.openxplay.stream.clink.ClinkPictureSize.entries.firstOrNull { it.name == bundle.getString("carlinkPictureSize") } ?: base.carlinkPictureSize,
        viewAreas = bundle.getBoolean("carPlayViewAreas", base.viewAreas),
        carBluetoothAudio = bundle.getBoolean("carBluetoothAudio", base.carBluetoothAudio),
        car = bundle.getStringArrayList("car")?.takeIf { it.size == 4 || it.size == 6 }?.let {
            if (it.size == 4) CarSelection(it[0], it[1], it[2], it[3]).normalized()
            else CarSelection(it[0], it[1], it[2], it[3], it[4], it[5])
        } ?: base.car,
        iphoneAddress = bundle.getString("iphoneAddress"),
        displayMode = migratedDisplayMode(bundle.getString("displayMode"), bundle.getIntArray("display")?.let { DisplayProfile(it[0], it[1], it[2]) }),
        display = bundle.getIntArray("display")?.let { DisplayProfile(it[0], it[1], it[2]) },
        wirelessNetwork = WirelessNetworkSettings(WirelessNetworkChoice.fromSavedName(text[8]), text[9], text[10],
            ManualHotspotBand.valueOf(text[11]), numbers[2], ManualHotspotSecurity.valueOf(text[12]), if (text.size > 14) SharedWifiNetworks.migrate(null, text[13], text[14]) else SharedWifiNetworks.decode(text[13])))
})
