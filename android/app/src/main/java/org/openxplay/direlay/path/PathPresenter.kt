// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-24
package org.openxplay.direlay.path

import java.time.Instant
import java.time.ZoneId
import java.time.format.DateTimeFormatter
import java.util.Locale
import org.openxplay.direlay.R
import org.openxplay.direlay.path.StepAction.*
import org.openxplay.direlay.path.StepId.*
import org.openxplay.direlay.path.StepState.*

/** A snapshot in, an immutable path out; elapsed time is rendered by the UI from sinceMillis. */
fun presentPath(input: PathInput, zone: ZoneId = ZoneId.systemDefault()): ConnectionPath {
    val running = input.iphone.started || input.iphone.armed || input.iphone.step !in setOf(LinkStep.IDLE, LinkStep.ENDED)
    // Read the pick before blocking: Bluetooth may be off when Connect arms a start.
    val canStart = input.setup.valid && input.relay.identity == IdentityState.READY &&
        input.relay.permissionsGranted >= input.relay.permissionsTotal && !input.relay.vpnConsentNeeded &&
        (!input.setup.wireless || input.iphone.pick == IphonePick.PICKED) && !running
    val ownSteps = input.stepIds().map { input.presentStep(it, canStart, running) }
    val prerequisites = prerequisites(ownSteps.map { it.id }, input.setup.network)
    val checked = linkedMapOf<StepId, PathStep>()
    ownSteps.forEach { step ->
        val blocked = prerequisites[step.id].orEmpty().any { id ->
            val previous = checked.getValue(id)
            previous.state != OK && !input.unreadableRadio(previous)
        }
        // A step that proved itself (OK), runs or failed keeps its own state; only waiting steps wait on others.
        checked[step.id] = if (blocked && step.state in setOf(WAITING, ACTION, BLOCKED)) step.copy(state = BLOCKED,
            detail = step.detail ?: StepText.of(R.string.path_after_step)) else step
    }
    val steps = checked.values.toList()
    val sections = PathSection.entries.mapNotNull { section ->
        steps.filter { it.id.section == section }.takeIf { it.isNotEmpty() }?.let { PathSectionView(section, it) }
    }
    val sessionRuns = input.iphone.step in setOf(LinkStep.SESSION, LinkStep.VIDEO)
    val live = input.live.pictureOnCar && sessionRuns
    val tone = when {
        live -> PathTone.LIVE
        steps.any { it.state == FAILED } -> PathTone.ERROR
        steps.any { it.state == ACTION } -> PathTone.ACTION
        running || steps.any { it.state == WORKING } -> PathTone.PROGRESS
        canStart -> PathTone.READY
        else -> PathTone.IDLE
    }
    val focus = steps.firstOrNull { it.state == FAILED || it.state == ACTION }
        ?: steps.firstOrNull { it.state == WORKING || it.state == WAITING }
        ?: steps.firstOrNull { it.state == BLOCKED }
    return ConnectionPath(sections, focus, tone, canStart, running,
        if (live) LiveSummary(input.live.sessionSinceMillis, input.live.chips.toList()) else null,
        PathNodes(input.iphone.name, input.relay.bluetoothName,
            input.car.carlink.carName ?: input.car.screen
                ?: input.car.address?.takeIf { CarOutput.BROWSER in input.setup.outputs } ?: input.car.dlna.name),
        diagnostic(sections, input, zone))
}

private fun PathInput.stepIds(): List<StepId> = buildList {
    addAll(listOf(MODE, IDENTITY, PERMISSIONS))
    if (relay.keepAlive in setOf(KeepAliveState.PENDING, KeepAliveState.REFUSED)) add(KEEP_ALIVE)
    if (setup.wireless || CarOutput.CARLINK in setup.outputs) add(BLUETOOTH)
    if (setup.network in setOf(NetworkPlan.WIFI_DIRECT, NetworkPlan.SHARED_WIFI) ||
        setup.outputs.any { it == CarOutput.CARLINK || it == CarOutput.MIRACAST }) add(WIFI)
    if (setup.network == NetworkPlan.PHONE_HOTSPOT) add(HOTSPOT)
    if (setup.network == NetworkPlan.SHARED_WIFI) add(SHARED_WIFI)
    if (CarOutput.BROWSER in setup.outputs) addAll(listOf(CAR_NETWORK, WEB_SERVER, CAR_PAGE))
    if (CarOutput.CARLINK in setup.outputs) addAll(listOf(CARLINK_FIND, CARLINK_LINK, CARLINK_PIN, CARLINK_READY))
    if (CarOutput.DLNA in setup.outputs) add(DLNA)
    if (CarOutput.MIRACAST in setup.outputs) add(MIRACAST)
    if (setup.wireless) addAll(listOf(IPHONE_PAIRED, CARPLAY_START, IPHONE_BLUETOOTH, IPHONE_AUTH, IPHONE_WIFI, SESSION))
    else addAll(listOf(USB_HOST, CARPLAY_START, USB_IPHONE, USB_TRUST, USB_CONTROL, SESSION))
    if (CarOutput.TESLA in setup.outputs) add(TESLA)
    add(PICTURE)
}

private fun prerequisites(ids: List<StepId>, network: NetworkPlan): Map<StepId, List<StepId>> {
    val carNetwork = when (network) {
        NetworkPlan.PHONE_HOTSPOT -> listOf(HOTSPOT)
        NetworkPlan.SHARED_WIFI -> listOf(SHARED_WIFI)
        else -> emptyList()
    }
    val iphone = ids.filter { it.section == PathSection.IPHONE }
    return buildMap {
        put(IDENTITY, listOf(MODE))
        put(PERMISSIONS, listOf(MODE))
        listOf(BLUETOOTH, WIFI, HOTSPOT, SHARED_WIFI).forEach { put(it, listOf(PERMISSIONS)) }
        put(CAR_NETWORK, carNetwork)
        put(CAR_PAGE, listOf(CAR_NETWORK, WEB_SERVER))
        put(CARLINK_FIND, listOf(BLUETOOTH, WIFI))
        put(CARLINK_LINK, listOf(CARLINK_FIND))
        put(CARLINK_PIN, listOf(CARLINK_LINK))
        put(CARLINK_READY, listOf(CARLINK_PIN))
        put(DLNA, if (CAR_NETWORK in ids) listOf(CAR_NETWORK) else listOf(WIFI, HOTSPOT).filter { it in ids })
        put(MIRACAST, listOf(WIFI))
        put(IPHONE_PAIRED, listOf(BLUETOOTH))
        iphone.zipWithNext().forEach { (before, after) -> put(after, listOf(before)) }
        put(CARPLAY_START, ids.filter { it.section == PathSection.RELAY } + iphone.first())
        put(TESLA, listOf(SESSION))
        put(PICTURE, listOf(SESSION))
    }
}

private fun PathInput.unreadableRadio(step: PathStep): Boolean = step.state == WAITING && when (step.id) {
    BLUETOOTH -> relay.bluetoothOn == null
    WIFI -> relay.wifiOn == null
    HOTSPOT -> relay.hotspotOn == null
    else -> false
}

private fun pathStep(id: StepId, state: StepState = BLOCKED, detail: StepText? = null,
    evidence: StepText? = null, code: String? = null, action: StepAction? = null, since: Long? = null,
    trail: List<String> = emptyList()): PathStep = PathStep(id, state, id.titleResource(),
    detail ?: if (state == BLOCKED) StepText.of(R.string.path_after_step) else null, evidence, code,
    action, action?.labelResource(), since, pathFailureExplanation(code) ?: id.explanationResource(), trail)

private fun raw(value: String?): StepText? = value?.let { StepText.raw(it) }
private fun text(res: Int, vararg args: String) = StepText.of(res, *args)

private fun PathInput.presentStep(id: StepId, canStart: Boolean, running: Boolean): PathStep {
    val own = when (id) {
        MODE -> modeStep()
        IDENTITY -> identityStep()
        PERMISSIONS -> permissionsStep()
        KEEP_ALIVE -> keepAliveStep()
        BLUETOOTH -> bluetoothStep()
        WIFI -> wifiStep()
        HOTSPOT -> hotspotStep()
        SHARED_WIFI -> sharedWifiStep()
        CAR_NETWORK -> carNetworkStep()
        WEB_SERVER -> webServerStep()
        CAR_PAGE -> carPageStep()
        CARLINK_FIND -> carlinkFindStep()
        CARLINK_LINK -> carlinkLinkStep()
        CARLINK_PIN -> carlinkPinStep()
        CARLINK_READY -> carlinkReadyStep()
        DLNA -> dlnaStep()
        MIRACAST -> miracastStep()
        IPHONE_PAIRED -> iphonePairedStep()
        USB_HOST -> usbHostStep()
        CARPLAY_START -> carplayStartStep(canStart)
        IPHONE_BLUETOOTH -> iphoneBluetoothStep()
        IPHONE_AUTH -> iphoneAuthStep()
        IPHONE_WIFI -> iphoneWifiStep()
        USB_IPHONE -> usbIphoneStep()
        USB_TRUST -> usbTrustStep()
        USB_CONTROL -> usbControlStep()
        SESSION -> sessionStep(running)
        TESLA -> teslaStep()
        PICTURE -> pictureStep()
    }
    val failure = iphone.failure?.takeIf { failureStep(it) == id } ?: return own
    val action = if (id == CARPLAY_START && failure.code.startsWith("hotspot_") && relay.hotspotOn == false)
        OPEN_HOTSPOT else RETRY
    val detail = if (failure.retrying) text(R.string.path_retrying, failure.code) else when (failure.code) {
        "first_tcp_timeout" -> text(R.string.path_iphone_did_not_join)
        else -> raw(failure.text)
    }
    return pathStep(id, if (failure.retrying) WORKING else FAILED, detail, own.evidence, failure.code,
        action, failure.atMillis, failure.trail)
}

private fun PathInput.modeStep() = if (setup.valid) pathStep(MODE, OK, evidence = raw(setup.summary))
    else pathStep(MODE, ACTION, text(R.string.path_settings_missing), action = FIX_SETTINGS)

private fun PathInput.identityStep() = when (relay.identity) {
    IdentityState.READY -> pathStep(IDENTITY, OK, evidence = raw(relay.identityLabel))
    IdentityState.CHECKING -> pathStep(IDENTITY, WORKING, text(R.string.path_checking))
    IdentityState.MISSING -> pathStep(IDENTITY, ACTION, text(R.string.path_import_identity), action = IMPORT_IDENTITY)
}

private fun PathInput.permissionsStep(): PathStep {
    val missing = (relay.permissionsTotal - relay.permissionsGranted).coerceAtLeast(0)
    return if (missing == 0 && !relay.vpnConsentNeeded) pathStep(PERMISSIONS, OK,
        evidence = raw("${relay.permissionsGranted}/${relay.permissionsTotal}"))
    else pathStep(PERMISSIONS, ACTION, if (relay.vpnConsentNeeded && missing <= 1) text(R.string.path_allow_vpn)
        else text(R.string.path_permissions_needed, missing.toString()), action = GRANT_PERMISSIONS)
}

private fun PathInput.keepAliveStep() = if (relay.keepAlive == KeepAliveState.PENDING)
    pathStep(KEEP_ALIVE, WORKING, text(R.string.path_keep_screen_open))
    else pathStep(KEEP_ALIVE, ACTION, text(R.string.path_background_stopped), action = KEEP_ALIVE_SETTINGS)

private fun PathInput.bluetoothStep() = when (relay.bluetoothOn) {
    true -> pathStep(BLUETOOTH, OK, evidence = raw(relay.bluetoothName) ?: text(R.string.path_on))
    false -> pathStep(BLUETOOTH, ACTION, text(R.string.path_turn_on_bluetooth), action = TURN_ON_BLUETOOTH)
    null -> pathStep(BLUETOOTH, WAITING, text(R.string.path_radio_unknown))
}

private fun PathInput.wifiStep() = when (relay.wifiOn) {
    true -> pathStep(WIFI, OK, evidence = raw(relay.wifiSsid) ?: text(R.string.path_on))
    false -> pathStep(WIFI, ACTION, text(R.string.path_turn_on_wifi), action = OPEN_WIFI)
    null -> pathStep(WIFI, WAITING, text(R.string.path_radio_unknown))
}

private fun PathInput.hotspotStep() = when (relay.hotspotOn) {
    true -> pathStep(HOTSPOT, OK, evidence = raw(listOfNotNull(relay.hotspotSsid, relay.hotspotBand).joinToString(" · ")))
    false -> pathStep(HOTSPOT, ACTION, text(R.string.path_turn_on_hotspot, relay.hotspotSsid.orEmpty()), action = OPEN_HOTSPOT)
    null -> pathStep(HOTSPOT, WAITING, text(R.string.path_radio_unknown))
}

private fun PathInput.sharedWifiStep() = if (relay.sharedWifiSsid != null)
    pathStep(SHARED_WIFI, OK, evidence = raw(relay.sharedWifiSsid))
    else pathStep(SHARED_WIFI, ACTION, text(R.string.path_join_shared_wifi), action = CHOOSE_SHARED_WIFI)

private fun PathInput.carNetworkStep() = if (car.address != null) pathStep(CAR_NETWORK, OK, evidence = raw(car.address))
    else pathStep(CAR_NETWORK, WAITING, text(R.string.path_car_network_wait))

private fun PathInput.webServerStep() = when (car.web.phase) {
    WebPhase.LISTENING -> pathStep(WEB_SERVER, OK,
        evidence = raw(car.web.trustedName ?: car.web.port?.let { ":$it HTTPS" }))
    WebPhase.OFF, WebPhase.STARTING -> pathStep(WEB_SERVER, WORKING, text(R.string.path_starting))
    WebPhase.FAILED -> pathStep(WEB_SERVER, FAILED, if (car.web.code == "web_port_busy")
        text(R.string.path_port_busy, car.web.port?.toString().orEmpty()) else text(R.string.path_could_not_start),
        code = car.web.code ?: "web_failed")
}

private fun PathInput.carPageStep() = if (car.pages > 0)
    pathStep(CAR_PAGE, OK, text(R.string.path_connected), raw(car.screen))
    else pathStep(CAR_PAGE, WAITING, text(R.string.path_open_car_page), raw(car.pageUrl), action = COPY_PAGE_ADDRESS)

private fun PathInput.carlinkFailureStep(): StepId? = car.carlink.code?.let { code -> when {
    code == "carlink_blocked" || code.startsWith("ble_") || code.startsWith("gatt_") -> CARLINK_FIND
    code == "projection_timeout" -> CARLINK_READY
    else -> CARLINK_LINK
} }

private fun PathInput.carlinkStep(id: StepId, state: StepState, detail: StepText? = null,
    evidence: StepText? = null, action: StepAction? = null): PathStep {
    val link = car.carlink
    val failed = carlinkFailureStep() == id
    return pathStep(id, if (failed) { if (link.retrying) WORKING else FAILED } else state,
        if (failed && link.retrying) text(R.string.path_retrying, link.code.orEmpty())
        else if (failed && pathFailureExplanation(link.code) == null && link.error != null) raw(link.error)
        else detail, evidence, if (failed) link.code else null,
        if (failed) { if (link.code == "carlink_blocked") FIX_SETTINGS else RECONNECT_CARLINK } else action,
        link.sinceMillis, link.trail)
}

private fun PathInput.carlinkPassed(id: StepId): Boolean {
    val order = listOf(CARLINK_FIND, CARLINK_LINK, CARLINK_PIN, CARLINK_READY)
    val current = carlinkFailureStep() ?: when (car.carlink.phase) {
        CarlinkPhase.OFF, CarlinkPhase.UNSUPPORTED, CarlinkPhase.NEEDS_PERMISSIONS,
        CarlinkPhase.BLOCKED, CarlinkPhase.SCANNING -> CARLINK_FIND
        CarlinkPhase.EXCHANGING, CarlinkPhase.JOINING, CarlinkPhase.AUTHENTICATING -> CARLINK_LINK
        CarlinkPhase.PIN -> CARLINK_PIN
        CarlinkPhase.NEGOTIATING, CarlinkPhase.PLAYING, CarlinkPhase.ENDED -> CARLINK_READY
    }
    return order.indexOf(id) < order.indexOf(current)
}

private fun PathInput.carlinkFindStep(): PathStep = when {
    car.carlink.phase == CarlinkPhase.UNSUPPORTED -> pathStep(CARLINK_FIND, FAILED,
        text(R.string.path_carlink_unsupported), since = car.carlink.sinceMillis)
    car.carlink.phase == CarlinkPhase.BLOCKED -> carlinkStep(CARLINK_FIND,
        if (car.carlink.retrying) WORKING else FAILED,
        if (car.carlink.retrying) text(R.string.path_retrying, "carlink_blocked") else text(R.string.path_carlink_blocked),
        action = FIX_SETTINGS).copy(code = "carlink_blocked", explanation = R.string.path_explain_carlink_blocked)
    car.carlink.phase == CarlinkPhase.NEEDS_PERMISSIONS -> pathStep(CARLINK_FIND, BLOCKED, text(R.string.path_after_step))
    carlinkFailureStep() == CARLINK_FIND -> carlinkStep(CARLINK_FIND, FAILED, text(R.string.path_car_not_found))
    carlinkPassed(CARLINK_FIND) -> carlinkStep(CARLINK_FIND, OK, evidence = raw(car.carlink.carName))
    else -> carlinkStep(CARLINK_FIND, WAITING, text(R.string.path_open_carlink))
}

private fun PathInput.carlinkLinkStep(): PathStep {
    val detail = when (car.carlink.phase) {
        CarlinkPhase.EXCHANGING -> text(R.string.path_exchanging)
        CarlinkPhase.JOINING -> text(R.string.path_joining_car_wifi)
        CarlinkPhase.AUTHENTICATING -> text(R.string.path_verifying)
        else -> null
    }
    return when {
        carlinkFailureStep() == CARLINK_LINK -> carlinkStep(CARLINK_LINK, FAILED, text(R.string.path_link_failed))
        carlinkPassed(CARLINK_LINK) -> carlinkStep(CARLINK_LINK, OK, evidence = raw(car.carlink.joinMethod))
        detail != null -> carlinkStep(CARLINK_LINK, WORKING, detail)
        else -> carlinkStep(CARLINK_LINK, BLOCKED)
    }
}

private fun PathInput.carlinkPinStep() = when {
    car.carlink.pinPending -> carlinkStep(CARLINK_PIN, ACTION, text(R.string.path_enter_pin), action = ENTER_PIN)
    car.carlink.trusted || carlinkPassed(CARLINK_PIN) -> carlinkStep(CARLINK_PIN, OK, text(R.string.path_trusted))
    else -> carlinkStep(CARLINK_PIN, BLOCKED)
}

private fun PathInput.carlinkReadyStep() = when {
    carlinkFailureStep() == CARLINK_READY -> carlinkStep(CARLINK_READY, FAILED, raw(car.carlink.code))
    car.carlink.phase == CarlinkPhase.NEGOTIATING -> carlinkStep(CARLINK_READY, WORKING, text(R.string.path_negotiating))
    car.carlink.phase == CarlinkPhase.PLAYING -> carlinkStep(CARLINK_READY, OK, evidence = raw(car.carlink.picture))
    car.carlink.phase == CarlinkPhase.ENDED -> carlinkStep(CARLINK_READY, WAITING,
        text(R.string.path_carlink_disconnected), action = RECONNECT_CARLINK)
    else -> carlinkStep(CARLINK_READY, BLOCKED)
}

private fun PathInput.dlnaStep() = when (car.dlna.phase) {
    OutputPhase.SEARCHING -> pathStep(DLNA, WORKING, text(R.string.path_searching), since = car.dlna.sinceMillis)
    OutputPhase.OFF, OutputPhase.WAITING_USER -> pathStep(DLNA, ACTION, text(R.string.path_choose_receiver), action = CHOOSE_RENDERER)
    OutputPhase.READY, OutputPhase.WAITING_VIDEO, OutputPhase.STARTING, OutputPhase.PLAYING ->
        pathStep(DLNA, OK, evidence = raw(car.dlna.name))
    OutputPhase.FAILED -> pathStep(DLNA, FAILED, raw(car.dlna.code), code = car.dlna.code,
        action = CHOOSE_RENDERER, since = car.dlna.sinceMillis)
}

private fun PathInput.miracastStep() = when (car.miracast.phase) {
    OutputPhase.PLAYING -> pathStep(MIRACAST, OK, evidence = raw(car.miracast.name))
    OutputPhase.FAILED -> pathStep(MIRACAST, FAILED, raw(car.miracast.code), code = car.miracast.code, since = car.miracast.sinceMillis)
    else -> pathStep(MIRACAST, WAITING, text(R.string.path_cast_to_car), action = OPEN_CAST, since = car.miracast.sinceMillis)
}

private fun PathInput.iphonePairedStep() = when (iphone.pick) {
    IphonePick.PICKED -> pathStep(IPHONE_PAIRED, OK, evidence = raw(iphone.name))
    IphonePick.NONE -> pathStep(IPHONE_PAIRED, ACTION, text(R.string.path_pair_iphone, relay.bluetoothName.orEmpty()), action = PAIR_IPHONE)
    IphonePick.UNPAIRED -> pathStep(IPHONE_PAIRED, ACTION, text(R.string.path_iphone_unpaired), action = CHOOSE_IPHONE)
    IphonePick.MULTIPLE -> pathStep(IPHONE_PAIRED, ACTION, text(R.string.path_choose_iphone), action = CHOOSE_IPHONE)
    IphonePick.NOT_IPHONE -> pathStep(IPHONE_PAIRED, ACTION, text(R.string.path_not_iphone, iphone.name.orEmpty()), action = CHOOSE_IPHONE)
}

private fun PathInput.usbHostStep() = when (iphone.usb) {
    UsbRole.HOST -> pathStep(USB_HOST, OK, text(R.string.path_usb_host_mode))
    UsbRole.DEVICE -> pathStep(USB_HOST, FAILED, text(R.string.path_usb_device_mode), code = "usb_role")
    UsbRole.DISCONNECTED, UsbRole.UNKNOWN -> pathStep(USB_HOST, WAITING, text(R.string.path_plug_iphone))
}

private fun PathInput.linkOrder() = if (setup.wireless)
    listOf(CARPLAY_START, IPHONE_BLUETOOTH, IPHONE_AUTH, IPHONE_WIFI, SESSION)
    else listOf(CARPLAY_START, USB_IPHONE, USB_TRUST, USB_CONTROL, SESSION)

private fun PathInput.linkStepId(step: LinkStep): StepId = when (step) {
    LinkStep.IDLE, LinkStep.STARTING -> CARPLAY_START
    LinkStep.WAITING_IPHONE, LinkStep.BLUETOOTH -> if (setup.wireless) IPHONE_BLUETOOTH else USB_IPHONE
    LinkStep.AUTH -> if (setup.wireless) IPHONE_AUTH else USB_CONTROL
    LinkStep.WIFI_SENT, LinkStep.JOINING -> if (setup.wireless) IPHONE_WIFI else USB_CONTROL
    LinkStep.USB_DETECT, LinkStep.USB_PERMISSION -> if (setup.wireless) IPHONE_BLUETOOTH else USB_IPHONE
    LinkStep.USB_TRUST -> if (setup.wireless) IPHONE_AUTH else USB_TRUST
    LinkStep.USB_CONTROL -> if (setup.wireless) IPHONE_AUTH else USB_CONTROL
    LinkStep.SESSION, LinkStep.VIDEO, LinkStep.ENDED -> SESSION
}

private fun PathInput.failureStep(failure: LinkFailure): StepId = when {
    failure.code.startsWith("hotspot_") || failure.code in setOf("wifi_off", "wifi_reset_required") -> CARPLAY_START
    setup.wireless && failure.code in setOf("bluetooth_off", "iphone_unpaired", "rfcomm_timeout", "bluetooth_unusable") -> IPHONE_BLUETOOTH
    setup.wireless && failure.code == "first_tcp_timeout" -> IPHONE_WIFI
    !setup.wireless && failure.code in setOf("usb_trust_denied", "iphone_locked") -> USB_TRUST
    !setup.wireless && failure.code == "usb_role" -> USB_HOST
    else -> linkStepId(failure.step)
}

private fun PathInput.linkProgress(id: StepId, state: StepState, detail: StepText? = null,
    evidence: StepText? = null): PathStep {
    val order = linkOrder()
    val current = if (!iphone.started && iphone.step in setOf(LinkStep.IDLE, LinkStep.ENDED)) -1
        else order.indexOf(linkStepId(iphone.step))
    val failed = iphone.failure?.let { order.indexOf(failureStep(it)) } ?: -1
    val progress = maxOf(current, failed)
    return when {
        order.indexOf(id) < progress -> pathStep(id, OK)
        order.indexOf(id) == progress -> pathStep(id, state, detail, evidence, since = iphone.stepSinceMillis)
        else -> pathStep(id, BLOCKED)
    }
}

private fun PathInput.carplayStartStep(canStart: Boolean): PathStep {
    iphone.carWaitUntilMillis?.let { until ->
        val remaining = (until - nowMillis).coerceAtLeast(0)
        val seconds = remaining / 1000 + if (remaining % 1000 > 0) 1 else 0
        return pathStep(CARPLAY_START, WAITING, text(R.string.path_car_screen_wait, seconds.toString()),
            action = START_NOW, since = iphone.stepSinceMillis)
    }
    if (iphone.armed) return pathStep(CARPLAY_START, BLOCKED, text(R.string.path_start_armed))
    if (iphone.step == LinkStep.IDLE && !iphone.started && iphone.failure == null)
        return pathStep(CARPLAY_START, BLOCKED, if (canStart) text(R.string.path_tap_connect) else null)
    return linkProgress(CARPLAY_START, WORKING, text(R.string.path_preparing))
}

private fun PathInput.iphoneBluetoothStep() = linkProgress(IPHONE_BLUETOOTH,
    if (iphone.step == LinkStep.WAITING_IPHONE) WAITING else WORKING,
    if (iphone.step == LinkStep.WAITING_IPHONE) text(R.string.path_iphone_near)
    else text(R.string.path_connecting_iphone, iphone.name.orEmpty()))

private fun PathInput.iphoneAuthStep() = linkProgress(IPHONE_AUTH, WORKING, evidence = raw(iphone.stepDetail))
private fun PathInput.iphoneWifiStep() = linkProgress(IPHONE_WIFI, WORKING,
    text(R.string.path_iphone_joining_wifi, relay.hotspotSsid ?: "Wi-Fi Direct"))
private fun PathInput.usbIphoneStep() = linkProgress(USB_IPHONE,
    if (iphone.step == LinkStep.USB_PERMISSION) ACTION else WAITING,
    text(if (iphone.step == LinkStep.USB_PERMISSION) R.string.path_allow_usb else R.string.path_plug_unlock_iphone))
private fun PathInput.usbTrustStep() = linkProgress(USB_TRUST, WAITING, text(R.string.path_trust_iphone))
private fun PathInput.usbControlStep() = linkProgress(USB_CONTROL, WORKING, text(R.string.path_connecting))

private fun PathInput.sessionStep(running: Boolean): PathStep = when {
    iphone.step == LinkStep.VIDEO -> pathStep(SESSION, OK, evidence = raw(live.chips.take(2).joinToString(" · ")))
    iphone.step == LinkStep.ENDED && running -> pathStep(SESSION, WAITING, text(R.string.path_reconnecting), since = iphone.stepSinceMillis)
    else -> linkProgress(SESSION, WORKING, text(R.string.path_waiting_picture))
}

private fun PathInput.teslaStep(): PathStep {
    if (iphone.step != LinkStep.VIDEO) return pathStep(TESLA, BLOCKED, text(R.string.path_tesla_after_session))
    return when (car.tesla.phase) {
        OutputPhase.FAILED -> pathStep(TESLA, FAILED, raw(car.tesla.code), code = car.tesla.code, since = car.tesla.sinceMillis)
        OutputPhase.READY, OutputPhase.PLAYING -> pathStep(TESLA, OK, evidence = raw(car.tesla.name))
        else -> pathStep(TESLA, BLOCKED, text(R.string.path_tesla_after_session))
    }
}

private fun PathInput.pictureStep() = when {
    iphone.step != LinkStep.VIDEO -> pathStep(PICTURE, BLOCKED, text(R.string.path_picture_after_session))
    live.pictureOnCar -> pathStep(PICTURE, OK, evidence = raw(live.chips.joinToString(" · ")))
    else -> pathStep(PICTURE, WAITING, text(R.string.path_waiting_car_picture), since = live.sessionSinceMillis)
}

private fun diagnostic(sections: List<PathSectionView>, input: PathInput, zone: ZoneId): String {
    val clock = DateTimeFormatter.ofPattern("HH:mm:ss", Locale.ROOT).withZone(zone)
    val parts = sections.map { section -> section.section.code + " " +
        section.steps.joinToString(" ") { it.id.code + it.state.glyph } }.toMutableList()
    sections.flatMap { it.steps }.firstOrNull { it.state == FAILED && it.code != null }?.let { failure ->
        parts.add("E ${failure.code} ${clock.format(Instant.ofEpochMilli(failure.sinceMillis ?: input.nowMillis))}")
    }
    parts.add("${input.version} ${clock.format(Instant.ofEpochMilli(input.nowMillis))}")
    return parts.joinToString(" | ")
}
