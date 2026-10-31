// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-17
package org.openxplay.direlay.ui

import androidx.compose.runtime.Immutable
import org.openxplay.direlay.R
import org.openxplay.direlay.path.*
import org.openxplay.direlay.settings.*

/** The guided setup's pages, in order. The network page exists only for wireless CarPlay. */
enum class SetupStep(val key: String) {
    CAR("car"), OUTPUT("output"), CONNECTION("connection"), NETWORK("network"), PREPARE("prepare"), START("start")
}
fun setupSteps(wireless: Boolean) = SetupStep.entries.filter { wireless || it != SetupStep.NETWORK }
fun setupStep(key: String) = SetupStep.entries.firstOrNull { it.key == key }

/** Wireless avoids the relay phone's USB power and cable limitations, including with Carlink. */
@Suppress("UNUSED_PARAMETER")
fun recommendedConnection(settings: RelaySettings) = ConnectionMode.WIRELESS

/** LAST_USED must be compared using the transport it would use now. */
fun needsUsbConfirmation(before: ConnectionMode, after: ConnectionMode, lastUsedWireless: Boolean): Boolean =
    after == ConnectionMode.USB && before.wireless(lastUsedWireless)

/** Carlink and Miracast need Wi-Fi Direct's radio for their link to the car. */
fun recommendedNetwork(settings: RelaySettings) =
    if (CarlinkConnectionPolicy.usesWifiDirect(settings)) WirelessNetworkChoice.PHONE_HOTSPOT else WirelessNetworkChoice.WIFI_DIRECT

/** The guide marks the recommended network with a badge, so Wi-Fi Direct drops "(recommended)" from its name. */
fun setupNetworkLabel(choice: WirelessNetworkChoice) =
    if (choice == WirelessNetworkChoice.WIFI_DIRECT) R.string.direlay_wifi_direct else choice.label

enum class SetupChoiceTag(val label: Int, val warning: Boolean = false) {
    RECOMMENDED(R.string.setup_recommended), EXPERIMENTAL(R.string.setup_experimental, true),
    COMING_SOON(R.string.setup_coming_soon, true), NOT_RECOMMENDED(R.string.setup_not_recommended, true)
}
fun setupOutputTag(choice: OutputChoice) = when (choice) {
    OutputChoice.BROWSER -> SetupChoiceTag.RECOMMENDED
    OutputChoice.CARLINK -> SetupChoiceTag.EXPERIMENTAL
    OutputChoice.WIRELESS -> SetupChoiceTag.COMING_SOON
}
/** Wireless display waits for device acceptance; Developer settings can still turn DLNA or Miracast on. */
fun outputOpen(choice: OutputChoice) = choice != OutputChoice.WIRELESS
fun setupConnectionTag(settings: RelaySettings, wireless: Boolean): SetupChoiceTag? = when {
    !wireless -> SetupChoiceTag.NOT_RECOMMENDED
    recommendedConnection(settings) == ConnectionMode.WIRELESS -> SetupChoiceTag.RECOMMENDED
    else -> null
}
fun setupNetworkTag(settings: RelaySettings, choice: WirelessNetworkChoice): SetupChoiceTag? =
    SetupChoiceTag.RECOMMENDED.takeIf { choice == recommendedNetwork(settings) }

/** The guide edits an explicit transport; LAST_USED resolves to the transport it would use now. */
fun setupDraft(settings: RelaySettings, wireless: Boolean) =
    if (settings.connectionMode != ConnectionMode.LAST_USED) settings
    else settings.copy(connectionMode = if (wireless) ConnectionMode.WIRELESS else ConnectionMode.USB)

/** One line of the chosen mode: what it is, the current choice, and the guide step that changes it. */
data class ModeRow(val title: Int, val value: Int, val icon: Int, val step: SetupStep)
fun modeRows(settings: RelaySettings, wireless: Boolean): List<ModeRow> = listOfNotNull(
    ModeRow(R.string.setup_summary_output, when (OutputSelection.choice(settings.outputs)) {
        OutputChoice.BROWSER -> R.string.ui_browser
        OutputChoice.CARLINK -> R.string.ui_carlink
        OutputChoice.WIRELESS -> if (OutputSelection.wirelessKind(settings.outputs) == WirelessDisplayKind.MIRACAST)
            R.string.wireless_miracast else R.string.wireless_dlna
        null -> R.string.output_multiple
    }, when (OutputSelection.choice(settings.outputs)) {
        OutputChoice.BROWSER -> R.drawable.symbol_monitor
        OutputChoice.CARLINK -> R.drawable.symbol_car_front
        OutputChoice.WIRELESS -> R.drawable.symbol_cast
        null -> R.drawable.symbol_link
    }, SetupStep.OUTPUT),
    ModeRow(R.string.ui_iphone_connection, if (wireless) R.string.direlay_wireless else R.string.ui_usb,
        if (wireless) R.drawable.symbol_wifi else R.drawable.symbol_phone, SetupStep.CONNECTION),
    if (!wireless) null else ModeRow(R.string.setup_summary_network, setupNetworkLabel(settings.wirelessNetwork.choice),
        R.drawable.symbol_router, SetupStep.NETWORK),
)

fun chooseOutput(settings: RelaySettings, choice: OutputChoice, kind: WirelessDisplayKind, wireless: Boolean): RelaySettings =
    OutputSelection.choose(settings, choice, kind, wireless)

/** The hotspot and shared Wi-Fi fields belong to the network page; they never block the other pages. */
private val networkFields = setOf("ssid", "password")
fun setupStepErrors(step: SetupStep, errors: Map<String, Int>, car: CarSelection = CarSelection()) = when (step) {
    SetupStep.CAR -> carSelectionErrors(car)
    SetupStep.NETWORK -> errors
    else -> errors - networkFields
}
fun chooseConnection(settings: RelaySettings, wireless: Boolean): RelaySettings {
    val candidate = settings.copy(connectionMode = if (wireless) ConnectionMode.WIRELESS else ConnectionMode.USB)
    return CarlinkConnectionPolicy.enabling(settings, candidate, wireless)
}

enum class SetupCheckState { DONE, TO_DO, CHECKING }
/** One thing to finish before CarPlay can start. [detail] and [action] come from the home checklist. */
data class SetupCheck(val title: Int, val state: SetupCheckState, val detail: Int? = null,
    val actionLabel: Int? = null, val action: SetupAction? = null)

/** Every check that applies to the chosen mode, done or not, derived from the home checklist. */
fun setupChecks(home: HomePresentation, wireless: Boolean, network: WirelessNetworkChoice): List<SetupCheck> {
    fun find(vararg actions: SetupAction) = home.checklist.firstOrNull { it.action in actions }
    fun check(title: Int, item: SetupItem?, detail: Int? = item?.text, checking: Boolean = false) = when {
        checking -> SetupCheck(title, SetupCheckState.CHECKING)
        item == null -> SetupCheck(title, SetupCheckState.DONE)
        else -> SetupCheck(title, SetupCheckState.TO_DO, detail, item.actionLabel, item.action)
    }
    return buildList {
        add(check(R.string.setup_check_permissions, find(SetupAction.PERMISSIONS), R.string.setup_check_permissions_hint))
        add(check(R.string.setup_check_identity, find(SetupAction.IDENTITY), R.string.setup_check_identity_hint, home.checking))
        find(SetupAction.SETTINGS)?.let { add(check(R.string.setup_check_settings, it)) }
        if (wireless && network == WirelessNetworkChoice.EXISTING_WIFI)
            add(check(R.string.setup_check_shared_wifi, find(SetupAction.SHARED_WIFI)))
        if (wireless) {
            // "Pair your iPhone" repeats the title; the screen shows where to pair instead.
            val iphone = find(SetupAction.IPHONE, SetupAction.CHOOSE_IPHONE)
            add(check(R.string.setup_check_iphone, iphone, iphone?.text?.takeIf { it != R.string.iphone_pair }))
        }
    }
}

fun compactSetup(heightDp: Int): Boolean = heightDp < 480

/** Preparation shows this phone's live checks and the iPhone prerequisite, before a session is started. */
fun setupPrepareSections(path: ConnectionPath) = listOfNotNull(
    path.section(PathSection.RELAY),
    path.section(PathSection.IPHONE)?.let { section ->
        section.copy(steps = section.steps.filter { it.id == StepId.IPHONE_PAIRED || it.id == StepId.USB_HOST })
    },
).filter { it.steps.isNotEmpty() }

@Immutable data class SetupPrepareSteps(val open: List<PathStep>, val done: List<PathStep>)
fun setupPrepareSteps(steps: List<PathStep>) = SetupPrepareSteps(
    steps.filter { it.state != StepState.OK }, steps.filter { it.state == StepState.OK })

@Immutable data class SetupPrepareChecks(val open: List<SetupCheck>, val done: List<SetupCheck>)
fun setupPrepareChecks(checks: List<SetupCheck>) = SetupPrepareChecks(
    checks.filter { it.state != SetupCheckState.DONE }, checks.filter { it.state == SetupCheckState.DONE })

fun setupPrepareComplete(path: ConnectionPath?, checks: List<SetupCheck>): Boolean = if (path != null) {
    val steps = setupPrepareSections(path).flatMap { it.steps }
    steps.isNotEmpty() && setupPrepareSteps(steps).open.isEmpty()
} else checks.isNotEmpty() && setupPrepareChecks(checks).open.isEmpty()

fun revealHotspotFields(network: WirelessNetworkSettings, errors: Map<String, Int>): Boolean =
    network.choice == WirelessNetworkChoice.PHONE_HOTSPOT &&
        (network.hotspotSetupMethod == HotspotSetupMethod.COPY_TO_SYSTEM || network.manualSsid.isBlank() ||
            network.manualPassphrase.isBlank() || errors.keys.any { it in networkFields })
