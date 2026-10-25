// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-14
package org.openxplay.direlay.ui

import org.openxplay.direlay.R
import org.openxplay.direlay.settings.RelaySettings
import org.openxplay.direlay.relay.WebStartReason
import org.openxplay.direlay.relay.RelayFailure
import org.openxplay.direlay.relay.CarPlayError
import org.openxplay.direlay.network.*
import org.openxplay.direlay.clink.ClinkFailureReason
import org.openxplay.direlay.path.*
import org.openxplay.direlay.vehicle.VehicleTileState
import org.openxplay.direlay.vehicle.VehicleView
import org.openxplay.direlay.vehicle.vehicleGlance
import org.openxplay.direlay.vehicle.vehicleTileState

/** No Android services or mutable runtime references enter the presentation mapper. */
enum class SetupAction { PERMISSIONS, IDENTITY, IPHONE, SETTINGS, SHARED_WIFI, CHOOSE_IPHONE }
internal fun NetworkRequirement.reminder() = when (this) {
    NetworkRequirement.SHARED_WIFI -> R.string.shared_wifi_unreadable
    NetworkRequirement.BLUETOOTH -> R.string.network_bluetooth_reminder
    NetworkRequirement.WIFI -> R.string.network_wifi_reminder
    NetworkRequirement.HOTSPOT -> R.string.network_hotspot_reminder
}
internal fun NetworkRequirement.settingsLabel() = when (this) {
    NetworkRequirement.SHARED_WIFI -> R.string.network_open_wifi
    NetworkRequirement.BLUETOOTH -> R.string.network_open_bluetooth
    NetworkRequirement.WIFI -> R.string.network_open_wifi
    NetworkRequirement.HOTSPOT -> R.string.network_open_hotspot
}
internal fun networkChoiceReminder(requirement: NetworkRequirement, readings: NetworkReadings): Int? =
    requirement.reminder().takeIf { when (requirement) {
        NetworkRequirement.SHARED_WIFI -> false
        NetworkRequirement.BLUETOOTH -> readings.bluetooth == null
        NetworkRequirement.WIFI -> readings.wifi == null
        NetworkRequirement.HOTSPOT -> readings.hotspot == null
    } }
data class SetupItem(val text: Int, val actionLabel: Int, val action: SetupAction)
/** Without text or a text resource, use the localized generic error. */
data class HomeProblem(val title: Int, val text: String? = null, val detail: String? = null,
    val textResource: Int? = null, val action: NetworkRequirement? = null, val arguments: List<String> = emptyList(),
    val atMillis: Long? = null)
/** Compatibility for the existing launch and setup screens; readiness comes only from the path. */
data class HomePresentation(val mode: List<ModeRow>, val checklist: List<SetupItem>,
    val canStart: Boolean, val running: Boolean, val checking: Boolean = false)

fun setupHomePresentation(input: PathInput, path: ConnectionPath, mode: List<ModeRow>): HomePresentation {
    val checklist = buildList {
        if (input.relay.permissionsGranted < input.relay.permissionsTotal || input.relay.vpnConsentNeeded || input.relay.overlayNeeded)
            add(SetupItem(R.string.ui_permissions, R.string.ui_allow, SetupAction.PERMISSIONS))
        if (input.relay.identity == IdentityState.MISSING)
            add(SetupItem(R.string.direlay_identity_required, R.string.direlay_import, SetupAction.IDENTITY))
        if (!input.setup.valid) add(SetupItem(R.string.ui_check_settings, R.string.direlay_settings, SetupAction.SETTINGS))
        if (input.setup.network == NetworkPlan.SHARED_WIFI && input.relay.sharedWifiSsid == null)
            add(SetupItem(if (input.relay.wifiSsid == null) R.string.shared_wifi_unreadable else R.string.shared_wifi_not_saved,
                R.string.shared_wifi_use, SetupAction.SHARED_WIFI))
        if (input.setup.wireless && input.iphone.pick != IphonePick.PICKED) {
            val pair = input.iphone.pick == IphonePick.NONE
            add(SetupItem(when (input.iphone.pick) {
                IphonePick.NONE -> R.string.iphone_pair
                IphonePick.UNPAIRED -> R.string.iphone_unpaired
                else -> R.string.iphone_multiple
            }, if (pair) R.string.ui_pair else R.string.iphone_choose,
                if (pair) SetupAction.IPHONE else SetupAction.CHOOSE_IPHONE))
        }
    }
    return HomePresentation(mode, checklist, path.canStart, path.running, input.relay.identity == IdentityState.CHECKING)
}

/** Radios may arm a start; preparation that needs the user must be finished first. */
fun missingStartStep(path: ConnectionPath): PathStep? = if (path.canStart || path.running) null else
    path.steps.firstOrNull { it.id in setOf(StepId.MODE, StepId.IDENTITY, StepId.PERMISSIONS, StepId.IPHONE_PAIRED) && it.state != StepState.OK }

/** The armed car-screen wait needs its Start now action even while the car page also waits. */
fun focusForPage(path: ConnectionPath): PathStep? = path.focus?.takeIf {
    it.state == StepState.FAILED || it.state == StepState.ACTION
} ?: path.steps.firstOrNull { it.action == StepAction.START_NOW } ?: path.focus

/** Idle waits stay in the focus card; disclosure is reserved for work and problems. */
fun expandPathSection(section: PathSectionView, running: Boolean): Boolean = section.steps.any {
    it.state in setOf(StepState.FAILED, StepState.ACTION, StepState.WORKING) || running && it.state == StepState.WAITING
}

/** Keep every address and the runtime's detailed failure evidence in the step sheet. */
fun stepDetails(step: PathStep, addresses: List<String>, problems: List<HomeProblem> = emptyList()): PathStep {
    val titles = when (step.id) {
        StepId.WEB_SERVER -> setOf(R.string.home_problem_web_server)
        StepId.DLNA -> setOf(R.string.home_problem_dlna)
        StepId.MIRACAST -> setOf(R.string.home_problem_miracast)
        StepId.TESLA -> setOf(R.string.tesla_title)
        else -> when (step.id.section) {
            PathSection.CAR -> setOf(R.string.home_problem_carlink, R.string.home_problem_browser)
            PathSection.IPHONE -> setOf(R.string.home_problem_carplay, R.string.home_problem_usb)
            else -> emptySet()
        }
    }
    val extra = problems.filter { it.title in titles }.flatMap { listOfNotNull(it.text, it.detail) }
    val urls = if (step.id == StepId.CAR_PAGE) addresses else emptyList()
    return step.copy(trail = (urls + step.trail + extra).filter { it.isNotBlank() }.distinct())
}

fun vehicleSummary(value: VehicleView, label: (Int, List<String>) -> String): String = when (vehicleTileState(value)) {
    VehicleTileState.OFF -> label(R.string.vehicle_off_value, emptyList())
    VehicleTileState.WAITING -> label(R.string.vehicle_waiting, emptyList())
    VehicleTileState.DATA -> label(R.string.vehicle_glance, vehicleGlance(value.reports))
}

fun visibleHomeFailures(settings: RelaySettings, failures: List<RelayFailure>): List<RelayFailure> = failures.filterNot {
    it.outputId == "cast" && !settings.dlnaOutput || it.outputId == "miracast" && !settings.miracastOutput
}

/** Runtime values are copied by the Activity; this mapper has no services or mutable runtime references. */
fun homeProblems(failures: List<RelayFailure>, carlinkFailed: Boolean, carlinkDetail: String,
    carlinkError: String?, webReason: WebStartReason?, serverState: String,
    readiness: NetworkReadiness = NetworkReadiness(), teslaFailure: String? = null,
    carlinkTrail: List<String> = emptyList(), carlinkErrorAt: Long? = null,
    carPlayError: CarPlayError? = null,
    dlnaFailure: org.openxplay.direlay.relay.WirelessDisplayFailure? = null,
    miracastFailure: org.openxplay.direlay.relay.WirelessDisplayFailure? = null): List<HomeProblem> = buildList {
    if (teslaFailure != null) add(HomeProblem(R.string.tesla_title,
        textResource = R.string.tesla_address_failed, arguments = listOf(teslaFailure)))
    carPlayError?.let { add(HomeProblem(R.string.home_problem_carplay, it.text,
        detail = it.trail.joinToString("\n"), textResource = carPlayFailureExplanation(it.code), atMillis = it.atMillis)) }
    val joinCodes = ClinkFailureReason.entries.filter { it.name.startsWith("P2P_") || it.name.startsWith("WIFI_") }.map { it.code }
    val carlinkFailure = carlinkFailed || failures.any { it.outputId in setOf("clink", "carlink") }
    val wifiProblem = carlinkFailure && (readiness.wifi == Readiness.PROBLEM || carlinkFailed && carlinkDetail in joinCodes)
    dlnaFailure?.let { add(wirelessDisplayProblem("cast", it)) }
    miracastFailure?.let { add(wirelessDisplayProblem("miracast", it)) }
    failures.forEach { failure ->
        if (failure.outputId == "cast" && dlnaFailure != null || failure.outputId == "miracast" && miracastFailure != null) return@forEach
        if (failure.outputId in setOf("clink", "carlink")) return@forEach
        if (failure.outputId == null && failure.text == carPlayError?.text) return@forEach
        add(HomeProblem(when (failure.outputId) {
            null -> R.string.home_problem_carplay
            "clink", "carlink" -> R.string.home_problem_carlink
            "web" -> R.string.home_problem_browser
            "cast" -> R.string.home_problem_dlna
            "miracast" -> R.string.home_problem_miracast
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
