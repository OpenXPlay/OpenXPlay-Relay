// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-24
package org.openxplay.direlay.relay

import org.openxplay.direlay.bluetooth.IphoneSelection
import org.openxplay.direlay.clink.ClinkStage
import org.openxplay.direlay.clink.WifiJoinPolicy
import org.openxplay.direlay.miracast.MiracastState
import org.openxplay.direlay.network.UsbConnectionState
import org.openxplay.direlay.path.*
import org.openxplay.direlay.settings.*
import org.openxplay.stream.cast.CastState
import org.openxplay.stream.core.VideoCodec
import org.openxplay.stream.core.VideoFormat
import java.util.Locale

/** Activity-owned facts, copied at the same time as the rest of Home's readiness snapshot. */
data class PathLocal(val permissionsGranted: Int = 0, val permissionsTotal: Int = 0,
    val vpnConsentNeeded: Boolean = false, val overlayNeeded: Boolean = false, val identity: IdentityState = IdentityState.CHECKING,
    val identityLabel: String? = null, val wifiSsid: String? = null, val bluetoothName: String? = null,
    val setupSummary: String = "", val nowMillis: Long = 0, val version: String = "", val setupValid: Boolean = true)

fun RelayRuntime.pathInput(local: PathLocal): PathInput {
    val settings = settingsStore.snapshot.value ?: return PathInput(nowMillis = local.nowMillis, version = local.version)
    val wireless = settingsStore.wireless(settings)
    val selection = iphoneSelection(settings.iphoneAddress)
    val link = iphoneLink.snapshot
    val clink = carlinkStatistics
    val phase = carlinkPhase(carlinkState, carlinkStage?.first, carlinkPins.pending != null)
    val carlinkSince = if (carlinkCode != null) carlinkErrorAt else when (phase) {
        CarlinkPhase.SCANNING, CarlinkPhase.EXCHANGING, CarlinkPhase.JOINING, CarlinkPhase.AUTHENTICATING, CarlinkPhase.PIN ->
            carlinkStage?.second ?: carlinkStateSinceMillis
        else -> carlinkStateSinceMillis
    }
    val dlna = dlnaPathInput(settings.dlnaOutput, settings.dlnaRenderer != null, dlnaState, dlnaRendererName, dlnaFailure, dlnaStateSinceMillis)
    val cast = miracastPathInput(settings.miracastOutput, miracastState, miracastDisplay?.name, miracastFailure, miracastStateSinceMillis)
    val video = pathVideoSnapshot()
    return PathInput(local.nowMillis, local.version,
        setup = SetupInput(if (!wireless) NetworkPlan.USB else when (settings.wirelessNetwork.choice) {
            WirelessNetworkChoice.WIFI_DIRECT -> NetworkPlan.WIFI_DIRECT
            WirelessNetworkChoice.PHONE_HOTSPOT -> NetworkPlan.PHONE_HOTSPOT
            WirelessNetworkChoice.EXISTING_WIFI -> NetworkPlan.SHARED_WIFI
        }, pathOutputs(settings), local.setupSummary, local.setupValid),
        relay = RelayInput(local.identity, local.identityLabel, local.permissionsGranted, local.permissionsTotal,
            local.vpnConsentNeeded, keepAlive.state, networkReadings.bluetooth, local.bluetoothName,
            networkReadings.wifi, local.wifiSsid, networkReadings.hotspot, settings.wirelessNetwork.manualSsid,
            hotspotBand(settings.wirelessNetwork.band), currentSharedWifi(settings, local.wifiSsid)?.ssid, local.overlayNeeded),
        car = CarInput(carIpv4.firstOrNull(), webPathInput(webPhase, settings.webPort ?: org.openxplay.direlay.tls.TlsStore.port(true), serverReason,
                publicName.status.value.trustedName, serverFailureSinceMillis), headUnitUrls().firstOrNull(), status.connectedWebPages,
            primaryCarScreen?.let { pictureSize(it.widthPx, it.heightPx) },
            savedCarScreen(settings)?.let { pictureSize(it.widthPx, it.heightPx) },
            CarlinkInput(phase, carlinkSince,
                if (phase == CarlinkPhase.BLOCKED) "carlink_blocked" else carlinkCode, carlinkError, carlinkReconnectPending,
                carlinkName, when (carlinkJoinMethod) {
                    WifiJoinPolicy.Method.P2P_CREDENTIALS -> "Wi-Fi Direct"
                    WifiJoinPolicy.Method.NETWORK_SPECIFIER -> "Wi-Fi"
                    else -> null
                }, carlinkTrusted, carlinkPins.pending != null,
                clink?.let { negotiatedPicture(it.width, it.height, it.codec) }, carlinkTrail()),
            dlna, cast, teslaPathInput(settings.teslaBrowser != TeslaBrowserMode.OFF, teslaUrl(), teslaFailure, teslaStateSinceMillis)),
        iphone = IphoneInput(iphonePick(selection), iphoneName(selection), when (networkReadings.usb) {
            UsbConnectionState.HOST -> UsbRole.HOST
            UsbConnectionState.DEVICE -> UsbRole.DEVICE
            UsbConnectionState.DISCONNECTED -> UsbRole.DISCONNECTED
            UsbConnectionState.UNKNOWN -> UsbRole.UNKNOWN
        }, autoStartSessions.tracker.running || startRequest.pending || sessionActions.restarting,
            networkStart.pending != null, carWaitUntilMillis, link.step, link.sinceMillis, link.detail, link.failure),
        live = LiveInput(status.session?.phase == org.openxplay.stream.core.SessionPhase.ACTIVE &&
            (status.webMeasurements().any { it.rendered > 0 } || (clink?.liveFrames ?: 0) > 0 ||
                dlna.phase == OutputPhase.PLAYING || cast.phase == OutputPhase.PLAYING),
            link.sessionSinceMillis, liveChips(status.videoFormat, video?.iphone as? VideoMeasurement.Values)))
}

internal fun iphonePick(selection: IphoneSelection): IphonePick = when (selection) {
    IphoneSelection.NoIphone -> IphonePick.NONE
    is IphoneSelection.SelectedUnpaired -> IphonePick.UNPAIRED
    is IphoneSelection.MultipleConnected, is IphoneSelection.MultipleCandidates -> IphonePick.MULTIPLE
    is IphoneSelection.OnlyBonded -> if (selection.device.strong) IphonePick.PICKED else IphonePick.NOT_IPHONE
    else -> IphonePick.PICKED
}

internal fun iphoneName(selection: IphoneSelection): String? = when (selection) {
    is IphoneSelection.Selected -> selection.device.name
    is IphoneSelection.Connected -> selection.device.name
    is IphoneSelection.OnlyCandidate -> selection.device.name
    is IphoneSelection.OnlyBonded -> selection.device.name
    else -> null
}

internal fun carlinkPhase(state: String, stage: ClinkStage?, pinPending: Boolean): CarlinkPhase = when (state) {
    "Disabled" -> CarlinkPhase.OFF
    "Requires Android 10+" -> CarlinkPhase.UNSUPPORTED
    "Waiting for permissions" -> CarlinkPhase.NEEDS_PERMISSIONS
    "Blocked" -> CarlinkPhase.BLOCKED
    "ENDED" -> CarlinkPhase.ENDED
    "PLAYING" -> CarlinkPhase.PLAYING
    "NEGOTIATING" -> CarlinkPhase.NEGOTIATING
    else -> when {
        pinPending -> CarlinkPhase.PIN
        stage in setOf(ClinkStage.AUTH, ClinkStage.AUTH_PINNED, ClinkStage.AUTH_SUCCESS, ClinkStage.TCP_CONNECTED) -> CarlinkPhase.AUTHENTICATING
        stage in setOf(ClinkStage.AUTH_PIN, ClinkStage.AUTH_CONFIRM) -> CarlinkPhase.PIN
        stage in setOf(ClinkStage.WIFI_JOIN, ClinkStage.TCP_CONNECT) -> CarlinkPhase.JOINING
        stage in setOf(ClinkStage.BLE_PEER, ClinkStage.GATT_CONNECT, ClinkStage.GATT_CONNECTED,
            ClinkStage.GATT_SERVICES, ClinkStage.BLE_EXCHANGE) -> CarlinkPhase.EXCHANGING
        state == "AUTHENTICATING" -> CarlinkPhase.AUTHENTICATING
        else -> CarlinkPhase.SCANNING
    }
}

internal fun webPathInput(phase: WebPhase, port: Int, reason: WebStartReason?, trustedName: String?, sinceMillis: Long? = null): WebServerInput =
    WebServerInput(phase, port, if (phase != WebPhase.FAILED) null else when (reason?.failure) {
        WebStartFailure.PORT_BUSY -> "web_port_busy"
        else -> "web_failed"
    }, trustedName, sinceMillis.takeIf { phase == WebPhase.FAILED })

internal fun dlnaPathInput(enabled: Boolean, selected: Boolean, state: CastState?, name: String?,
    failure: WirelessDisplayFailure?, sinceMillis: Long? = null): OutputInput = OutputInput(when {
    !enabled -> OutputPhase.OFF
    failure != null -> OutputPhase.FAILED
    !selected -> OutputPhase.WAITING_USER
    else -> when (state) {
        CastState.SEARCHING -> OutputPhase.SEARCHING
        CastState.WAITING_FOR_VIDEO -> OutputPhase.WAITING_VIDEO
        CastState.STARTING -> OutputPhase.STARTING
        CastState.PLAYING -> OutputPhase.PLAYING
        else -> OutputPhase.READY
    }
}, name, failure?.code.takeIf { enabled }, (failure?.atMillis ?: sinceMillis).takeIf { enabled })

internal fun miracastPathInput(enabled: Boolean, state: MiracastState?, name: String?,
    failure: WirelessDisplayFailure?, sinceMillis: Long? = null): OutputInput = OutputInput(when {
    !enabled -> OutputPhase.OFF
    failure != null -> OutputPhase.FAILED
    state == MiracastState.SHOWING -> OutputPhase.PLAYING
    else -> OutputPhase.WAITING_USER
}, name, failure?.code.takeIf { enabled }, (failure?.atMillis ?: sinceMillis).takeIf { enabled })

internal fun teslaPathInput(enabled: Boolean, url: String?, failure: String?, sinceMillis: Long? = null): OutputInput = OutputInput(when {
    !enabled -> OutputPhase.OFF
    failure != null -> OutputPhase.FAILED
    url != null -> OutputPhase.READY
    else -> OutputPhase.STARTING
}, url, failure.takeIf { enabled }, sinceMillis.takeIf { enabled })

internal fun pictureSize(width: Int, height: Int): String? = if (width > 0 && height > 0) "$width×$height" else null
internal fun codecLabel(codec: VideoCodec): String = when (codec) { VideoCodec.H264 -> "H.264"; VideoCodec.H265 -> "H.265" }
internal fun negotiatedPicture(width: Int, height: Int, codec: VideoCodec?): String? =
    pictureSize(width, height)?.let { size -> listOfNotNull(size, codec?.let(::codecLabel)).joinToString(" ") }
internal fun hotspotBand(band: com.shilapi.xcertplay.orchestration.ManualHotspotBand): String? = when (band) {
    com.shilapi.xcertplay.orchestration.ManualHotspotBand.GHZ_2_4 -> "2.4 GHz"
    com.shilapi.xcertplay.orchestration.ManualHotspotBand.GHZ_5 -> "5 GHz"
    else -> null
}
internal fun liveChips(format: VideoFormat?, values: VideoMeasurement.Values?): List<String> = buildList {
    format?.let { pictureSize(it.width, it.height)?.let(::add) }
    (values?.fps ?: format?.fpsHint)?.takeIf { it.isFinite() && it > 0 }?.let {
        add(String.format(Locale.ROOT, "%.0f fps", it))
    }
    format?.codec?.let { add(codecLabel(it)) }
    values?.bitsPerSecond?.takeIf { it.isFinite() && it > 0 }?.let { add(String.format(Locale.ROOT, "%.1f Mbps", it / 1_000_000)) }
}

internal fun pathOutputs(settings: RelaySettings): Set<CarOutput> = settings.outputs.mapTo(linkedSetOf()) { when (it) {
    RelayOutput.BROWSER -> CarOutput.BROWSER
    RelayOutput.CARLINK -> CarOutput.CARLINK
    RelayOutput.DLNA -> CarOutput.DLNA
    RelayOutput.MIRACAST -> CarOutput.MIRACAST
} }.apply { if (settings.teslaBrowser != TeslaBrowserMode.OFF) add(CarOutput.TESLA) }
