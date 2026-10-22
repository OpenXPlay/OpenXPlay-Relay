// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-14
package org.openxplay.direlay.ui

import org.openxplay.direlay.bluetooth.IphoneSelection
import org.openxplay.direlay.R
import org.openxplay.direlay.settings.RelaySettings
import org.openxplay.direlay.relay.WebStartReason
import org.openxplay.direlay.relay.WebStartFailure
import org.openxplay.direlay.relay.RelayFailure
import org.openxplay.direlay.relay.CarPlayError
import org.openxplay.direlay.network.*
import org.openxplay.direlay.clink.ClinkFailureReason
import org.openxplay.stream.core.SessionPhase
import org.openxplay.direlay.vehicle.VehicleTileState
import org.openxplay.direlay.vehicle.VehicleView
import org.openxplay.direlay.vehicle.vehicleGlance
import org.openxplay.direlay.vehicle.vehicleTileState

/** No Android services or mutable runtime references enter the presentation mapper. */
enum class StatusTone { IDLE, PROGRESS, ATTENTION, RUNNING, ERROR }
enum class SetupAction { PERMISSIONS, IDENTITY, IPHONE, SETTINGS, SHARED_WIFI, CHOOSE_IPHONE }
internal fun NetworkRequirement.reminder() = when (this) {
    NetworkRequirement.WIFI -> R.string.network_wifi_reminder
    NetworkRequirement.HOTSPOT -> R.string.network_hotspot_reminder
}
internal fun NetworkRequirement.problem() = when (this) {
    NetworkRequirement.WIFI -> R.string.network_wifi_problem
    NetworkRequirement.HOTSPOT -> R.string.network_hotspot_problem
}
internal fun NetworkRequirement.settingsLabel() = when (this) {
    NetworkRequirement.WIFI -> R.string.network_open_wifi
    NetworkRequirement.HOTSPOT -> R.string.network_open_hotspot
}
internal fun networkChoiceReminder(requirement: NetworkRequirement, readings: NetworkReadings): Int? =
    requirement.reminder().takeIf { when (requirement) {
        NetworkRequirement.WIFI -> readings.wifi == null
        NetworkRequirement.HOTSPOT -> readings.hotspot == null
    } }
data class SetupItem(val text: Int, val actionLabel: Int, val action: SetupAction)
/** Without text or a text resource, use the localized generic error. */
data class HomeProblem(val title: Int, val text: String? = null, val detail: String? = null,
    val textResource: Int? = null, val action: NetworkRequirement? = null, val arguments: List<String> = emptyList(),
    val atMillis: Long? = null)
data class HomePresentation(val headline: Int, val subline: Int, val tone: StatusTone,
    val mode: List<ModeRow>, val checklist: List<SetupItem>, val canStart: Boolean, val running: Boolean,
    val checking: Boolean = false, val sublineArguments: List<String> = emptyList(),
    val problems: List<HomeProblem> = emptyList(), val hints: List<HomeProblem> = emptyList(),
    val reminders: List<Int> = emptyList())
data class HomeSnapshot(
    val settings: RelaySettings, val wireless: Boolean, val permissionsReady: Boolean,
    val identityReady: Boolean, val checking: Boolean = false, val validSettings: Boolean = true,
    val phase: SessionPhase? = null, val started: Boolean = false, val failure: Boolean = false,
    val iphoneSelection: IphoneSelection = IphoneSelection.NoIphone, val autoStartPaused: Boolean = false,
    val sharedWifiReady: Boolean = true, val currentWifi: String? = null,
    val webReason: WebStartReason? = null, val webListening: Boolean = false, val carNetworkAvailable: Boolean = true,
    val problems: List<HomeProblem> = emptyList(),
    val networkReadiness: NetworkReadiness = NetworkReadiness(),
    val manualNetworkReminders: Set<NetworkRequirement> = emptySet(), val usbRoleProblem: Boolean = false,
)

fun connectionSummary(mode: List<ModeRow>, carlinkState: Int?, label: (Int) -> String): String =
    listOfNotNull(mode.firstOrNull { it.step == SetupStep.CONNECTION }?.value,
        mode.firstOrNull { it.step == SetupStep.OUTPUT }?.value, carlinkState)
        .joinToString(" · ", transform = label)

fun networkSummary(wifi: String, addresses: String): String =
    listOfNotNull(wifi.trim().takeIf { it.isNotEmpty() },
        addresses.lineSequence().map { it.trim() }.firstOrNull { it.isNotEmpty() })
        .distinct().joinToString(" · ")

fun vehicleSummary(value: VehicleView, label: (Int, List<String>) -> String): String = when (vehicleTileState(value)) {
    VehicleTileState.OFF -> label(R.string.vehicle_off_value, emptyList())
    VehicleTileState.WAITING -> label(R.string.vehicle_waiting, emptyList())
    VehicleTileState.DATA -> label(R.string.vehicle_glance, vehicleGlance(value.reports))
}

fun carlinkStateLabel(state: String, failed: Boolean, awaitingPin: Boolean): Int = when {
    awaitingPin -> R.string.ui_waiting_code
    state in setOf("ACTIVE", "CONNECTED", "STREAMING", "PLAYING") -> R.string.ui_connected
    state == "Requires Android 10+" -> R.string.ui_carlink_unsupported
    failed || state == "ENDED" -> R.string.ui_carlink_failed
    else -> R.string.ui_searching
}

/** Runtime values are copied by the Activity; this mapper has no services or mutable runtime references. */
fun homeProblems(failures: List<RelayFailure>, carlinkFailed: Boolean, carlinkDetail: String,
    carlinkError: String?, webReason: WebStartReason?, serverState: String,
    readiness: NetworkReadiness = NetworkReadiness(), teslaFailure: String? = null,
    carlinkTrail: List<String> = emptyList(), carlinkErrorAt: Long? = null,
    carPlayError: CarPlayError? = null): List<HomeProblem> = buildList {
    if (teslaFailure != null) add(HomeProblem(R.string.tesla_title,
        textResource = R.string.tesla_address_failed, arguments = listOf(teslaFailure)))
    carPlayError?.let { add(HomeProblem(R.string.home_problem_carplay, it.text,
        detail = it.trail.joinToString("\n"), textResource = carPlayFailureExplanation(it.code), atMillis = it.atMillis)) }
    val joinCodes = ClinkFailureReason.entries.filter { it.name.startsWith("P2P_") || it.name.startsWith("WIFI_") }.map { it.code }
    val carlinkFailure = carlinkFailed || failures.any { it.outputId in setOf("clink", "carlink") }
    val wifiProblem = carlinkFailure && (readiness.wifi == Readiness.PROBLEM || carlinkFailed && carlinkDetail in joinCodes)
    failures.forEach { failure ->
        if (failure.outputId in setOf("clink", "carlink")) return@forEach
        if (failure.outputId == null && failure.text == carPlayError?.text) return@forEach
        add(HomeProblem(when (failure.outputId) {
            null -> R.string.home_problem_carplay
            "clink", "carlink" -> R.string.home_problem_carlink
            "web" -> R.string.home_problem_browser
            else -> R.string.home_problem_output
        }, failure.text, atMillis = failure.atMillis.takeIf { it != 0L }))
    }
    if (carlinkFailure) {
        val outputErrors = failures.filter { it.outputId in setOf("clink", "carlink") }
        val error = carlinkError?.takeIf { it.isNotBlank() } ?: outputErrors.firstOrNull()?.text
        // While Carlink is healthy the detail holds video status text, not a failure code.
        val code = carlinkDetail.takeIf { carlinkFailed && it.isNotBlank() }
        val text = when {
            error == null -> code
            code != null && error.lineSequence().first() != code -> "$code\n$error"
            else -> error
        }
        val otherErrors = outputErrors.map { it.text }.filter { it != error }.distinct().joinToString("\n")
        val trail = carlinkTrail.joinToString("\n")
        val at = carlinkErrorAt ?: outputErrors.firstOrNull()?.atMillis?.takeIf { it != 0L }
        if (wifiProblem) add(HomeProblem(R.string.home_problem_carlink,
            detail = listOf((listOfNotNull(code, carlinkError?.takeIf { it.isNotBlank() }) +
                outputErrors.map { it.text }).distinct().joinToString("\n"), trail)
                .filter { it.isNotBlank() }.joinToString("\n\n"),
            textResource = R.string.network_wifi_problem, action = NetworkRequirement.WIFI, atMillis = at))
        else add(HomeProblem(R.string.home_problem_carlink, text,
            listOf(otherErrors, trail).filter { it.isNotBlank() }.joinToString("\n\n").takeIf { it.isNotBlank() },
            carlinkFailureExplanation((code ?: error.orEmpty()).lineSequence().first()), atMillis = at))
    }
    if ((carlinkFailure || failures.any { it.outputId == null }) && readiness.hotspot == Readiness.PROBLEM)
        add(HomeProblem(R.string.home_problem_hotspot, textResource = R.string.network_hotspot_problem,
            action = NetworkRequirement.HOTSPOT))
    if (webReason != null) add(HomeProblem(R.string.home_problem_web_server,
        serverState.takeIf { it.isNotBlank() }, webReason.toString()))
}

fun carPlayFailureExplanation(code: String?): Int? = when (code) {
    "hotspot_not_ready" -> R.string.error_explain_hotspot_not_ready
    "hotspot_configuration" -> R.string.error_explain_hotspot_configuration
    "first_tcp_timeout" -> R.string.error_explain_first_tcp_timeout
    "bluetooth_unusable" -> R.string.error_explain_bluetooth_unusable
    else -> null
}

fun presentHome(value: HomeSnapshot): HomePresentation {
    val browser = !value.settings.carlinkOutput || value.settings.bothOutputs
    val webReason = value.webReason.takeIf { browser }
    val iphoneReady = !value.wireless || value.iphoneSelection.address != null
    val checklist = buildList {
        if (!value.permissionsReady) add(SetupItem(R.string.ui_permissions, R.string.ui_allow, SetupAction.PERMISSIONS))
        if (!value.identityReady && !value.checking) add(SetupItem(R.string.direlay_identity_required, R.string.direlay_import, SetupAction.IDENTITY))
        if (!value.validSettings) add(SetupItem(R.string.ui_check_settings, R.string.direlay_settings, SetupAction.SETTINGS))
        if (!value.sharedWifiReady) add(SetupItem(
            if (value.currentWifi == null) R.string.shared_wifi_unreadable else R.string.shared_wifi_not_saved,
            if (value.currentWifi == null) R.string.shared_wifi_use else R.string.shared_wifi_save_current, SetupAction.SHARED_WIFI))
        if (!iphoneReady) {
            val text = when (value.iphoneSelection) {
                is IphoneSelection.SelectedUnpaired -> R.string.iphone_unpaired
                is IphoneSelection.MultipleCandidates,
                is IphoneSelection.MultipleConnected -> R.string.iphone_multiple
                else -> R.string.iphone_pair
            }
            add(SetupItem(text, if (text == R.string.iphone_pair) R.string.ui_pair else R.string.iphone_choose,
                if (text == R.string.iphone_pair) SetupAction.IPHONE else SetupAction.CHOOSE_IPHONE))
        }
    }
    val ready = value.permissionsReady && value.identityReady && value.validSettings && value.sharedWifiReady && !value.checking
    val active = value.phase == SessionPhase.ACTIVE
    val running = active || value.phase in setOf(SessionPhase.CONNECTING, SessionPhase.RECONNECTING) || value.started
    val (headline, tone) = when {
        value.failure || webReason != null -> R.string.ui_error to StatusTone.ERROR
        !ready -> R.string.ui_setup_needed to StatusTone.ATTENTION
        active -> R.string.ui_running to StatusTone.RUNNING
        value.phase in setOf(SessionPhase.CONNECTING, SessionPhase.RECONNECTING) -> R.string.ui_connecting to StatusTone.PROGRESS
        value.started -> R.string.ui_waiting to StatusTone.PROGRESS
        !iphoneReady -> R.string.ui_setup_needed to StatusTone.ATTENTION
        else -> R.string.ui_ready to StatusTone.IDLE
    }
    val subline = when {
        webReason?.failure == WebStartFailure.PORT_BUSY -> R.string.ui_web_port_busy
        webReason != null -> R.string.ui_web_start_failed
        value.failure -> R.string.ui_retry_hint
        value.checking -> R.string.direlay_checking_identity
        !ready -> R.string.ui_setup_hint
        browser && value.webListening && !value.carNetworkAvailable -> R.string.ui_head_unit_network
        active && value.settings.bothOutputs -> R.string.ui_showing_both
        active && value.settings.carlinkOutput -> R.string.ui_showing_carlink
        active -> R.string.ui_showing_browser
        value.phase in setOf(SessionPhase.CONNECTING, SessionPhase.RECONNECTING) -> R.string.ui_connecting_hint
        value.started -> if (value.wireless) R.string.ui_iphone_pairing else R.string.ui_usb_hint
        value.settings.autoStartWhenReady && value.autoStartPaused && !running -> R.string.ui_automatic_paused
        else -> R.string.ui_start_hint
    }
    return HomePresentation(headline, subline, tone, modeRows(value.settings, value.wireless),
        checklist, ready && iphoneReady, running, value.checking,
        if (webReason?.failure == WebStartFailure.PORT_BUSY) listOf(webReason.port.toString()) else emptyList(),
        if (tone == StatusTone.ERROR) value.problems.ifEmpty { listOf(HomeProblem(R.string.home_problem_connection)) } else emptyList(),
        (if (tone != StatusTone.ERROR) value.problems.filter { it.title == R.string.tesla_title } else emptyList()) +
            (if (running && !active && !value.wireless && value.usbRoleProblem) listOf(HomeProblem(R.string.home_problem_usb,
                detail = "USB_STATE: connected=true, host_connected=false; USB devices=[]", textResource = R.string.network_usb_role_problem)) else emptyList()),
        if (tone == StatusTone.PROGRESS) (value.manualNetworkReminders intersect value.networkReadiness.unknown)
            .map { it.reminder() } else emptyList())
}

fun carlinkFailureExplanation(code: String): Int? = when (code) {
    "ble_unavailable" -> R.string.error_explain_ble_unavailable
    "ble_timeout" -> R.string.error_explain_ble_timeout
    "ble_scan_failed" -> R.string.error_explain_ble_scan_failed
    "gatt_connect_failed" -> R.string.error_explain_gatt_connect_failed
    "gatt_services_failed", "ble_discovery_failed" -> R.string.error_explain_gatt_services_failed
    "ble_service_missing" -> R.string.error_explain_ble_service_missing
    "ble_subscribe_failed" -> R.string.error_explain_ble_subscribe_failed
    "ble_cccd_failed" -> R.string.error_explain_ble_cccd_failed
    "ble_cccd_timeout" -> R.string.error_explain_ble_cccd_timeout
    "ble_write_failed" -> R.string.error_explain_ble_write_failed
    "ble_request_rejected" -> R.string.error_explain_ble_request_rejected
    "ble_reply_invalid" -> R.string.error_explain_ble_reply_invalid
    "ble_verify_timeout" -> R.string.error_explain_ble_verify_timeout
    "ble_exchange_timeout" -> R.string.error_explain_ble_exchange_timeout
    "ble_exchange_failed" -> R.string.error_explain_ble_exchange_failed
    else -> null
}
