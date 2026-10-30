// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-25
package org.openxplay.direlay.presentation

import kotlinx.coroutines.flow.StateFlow
import org.openxplay.direlay.R
import org.openxplay.direlay.background.BackgroundDestination
import org.openxplay.direlay.background.BackgroundStatus
import org.openxplay.direlay.relay.*
import org.openxplay.direlay.settings.*
import org.openxplay.direlay.publicname.*
import org.openxplay.direlay.ui.*
import org.openxplay.stream.core.Viewport

data class SettingsView(val settings: RelaySettings, val overrides: DeveloperOverrides,
    val setupCompleted: Boolean, val lastWireless: Boolean, val models: CarModels,
    val version: String, val environment: String, val adminEnabled: Boolean, val publicNameEnabled: Boolean, val appliedFps: Int, val negotiatedFps: Double?, val sampleVehicle: Boolean)

data class DetectedCarScreen(val screen: ScreenDataKey, val records: List<WindowSizeRecord>, val receiver: String?)
data class CarScreenView(val models: CarModels? = null, val detected: DetectedCarScreen? = null,
    val savedCanvas: CarPlayScreen? = null, val version: String = "", val phoneAddresses: Set<String> = emptySet(),
    val publicName: PublicNameView = PublicNameView())
data class WirelessPictureView(val automatic: WirelessPictureResolution? = null, val negotiated: Viewport? = null,
    val overlayAllowed: Boolean = false)
data class AreaSwitchView(val time: Long, val index: Int, val accepted: Boolean)
data class ViewAreasView(val areas: List<ViewAreaSize> = emptyList(), val preference: Boolean? = null,
    val records: List<WindowSizeRecord> = emptyList(), val exits: List<ReceiverDiagnostics.FullscreenExit> = emptyList(),
    val log: List<AreaSwitchView> = emptyList())
data class TraceLineView(val type: String, val json: String)
data class CarlinkDebugView(val lines: List<TraceLineView> = emptyList(), val dropped: Long = 0,
    val file: java.io.File? = null, val failure: String? = null, val capturing: Boolean = false, val bytes: Long = 0)
data class AdminView(val enabled: Boolean = false, val urls: List<String> = emptyList())
data class NameView(val view: PublicNameView = PublicNameView(), val status: PublicNameStatus = PublicNameStatus(PublicNameState.OFF),
    val url: String? = null)
data class IphonePickerView(val devices: List<org.openxplay.direlay.bluetooth.BondedDevice> = emptyList(), val localName: String? = null)
data class AppliedSettings(val before: RelaySettings, val after: RelaySettings, val feedback: SettingsFeedback?)

class ScreenSources(val motion: StateFlow<Boolean>, val settings: StateFlow<SettingsView?>, val car: StateFlow<CarScreenView>,
    val wirelessPicture: StateFlow<WirelessPictureView>, val areas: StateFlow<ViewAreasView>,
    val debug: StateFlow<CarlinkDebugView>, val developer: StateFlow<DeveloperView>,
    val identity: StateFlow<Int>, val admin: StateFlow<AdminView>, val name: StateFlow<NameView>,
    val iphones: StateFlow<IphonePickerView>, val carlinkPicture: StateFlow<CarlinkPictureView>,
    val background: StateFlow<BackgroundStatus?>)

/** The size tried for this connection, the canvas it produced and the size the running connection uses. */
data class CarlinkPictureView(val trial: CarlinkPictureTrial? = null, val canvas: PictureSize? = null,
    val effective: CarlinkPictureTrial? = null)

class ScreenActions(val save: suspend (RelaySettings) -> AppliedSettings?,
    val saveDeveloper: suspend (RelaySettings, DeveloperOverrides) -> AppliedSettings?,
    val readIphones: suspend () -> IphonePickerView, val completeSetup: () -> Unit, val draft: (RelaySettings) -> Unit,
    val clearHeadUnit: () -> Unit, val showArea: (Int) -> Unit, val clearWindows: () -> Unit,
    val clearTrace: () -> Unit, val capture: (Boolean) -> Unit,
    val searchRenderers: suspend () -> List<org.openxplay.stream.cast.CastRenderer>,
    val adminEnabled: (Boolean) -> Unit, val resetPassword: suspend () -> Unit,
    val saveName: suspend (PublicNameEdit) -> List<ConfigError>?, val renewName: () -> Unit,
    val removeName: suspend () -> Boolean, val useSharedWifi: (String) -> Unit,
    val sampleVehicle: (Boolean) -> Unit, val clearVehicleHistory: () -> Unit,
    val tryCarlinkPicture: (PictureSize?) -> Unit, val clearCarlinkPictureTrial: (restart: Boolean) -> Unit,
    val refreshBackground: () -> Unit, val openBackground: (BackgroundDestination) -> Unit)
