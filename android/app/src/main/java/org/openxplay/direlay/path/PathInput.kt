// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-24
package org.openxplay.direlay.path

/**
 * Everything the connection path is computed from, copied from the runtime on the status tick.
 * Plain values only: no Android types, services or mutable runtime references, so [presentPath] stays pure.
 * Wall-clock times are epoch milliseconds; null means "not known".
 */
data class PathInput(
    val nowMillis: Long = 0,
    val version: String = "",
    val setup: SetupInput = SetupInput(),
    val relay: RelayInput = RelayInput(),
    val car: CarInput = CarInput(),
    val iphone: IphoneInput = IphoneInput(),
    val live: LiveInput = LiveInput(),
)

/** How the iPhone reaches the relay; USB is the wired transport. */
enum class NetworkPlan { WIFI_DIRECT, PHONE_HOTSPOT, SHARED_WIFI, USB }

/** Outputs towards the car; TESLA is the browser output through the Tesla address. */
enum class CarOutput { BROWSER, CARLINK, DLNA, MIRACAST, TESLA }

data class SetupInput(
    val network: NetworkPlan = NetworkPlan.WIFI_DIRECT,
    val outputs: Set<CarOutput> = setOf(CarOutput.BROWSER),
    /** Already localized, e.g. "Wireless · Hotspot · Browser". */
    val summary: String = "",
    val valid: Boolean = true,
) {
    val wireless get() = network != NetworkPlan.USB
}

enum class IdentityState { READY, MISSING, CHECKING }
enum class KeepAliveState { NOT_NEEDED, PENDING, FOREGROUND, REFUSED }

data class RelayInput(
    val identity: IdentityState = IdentityState.CHECKING,
    /** Where the MFi identity comes from when ready: a localized "Installed", "Remote", "I²C". */
    val identityLabel: String? = null,
    val permissionsGranted: Int = 0,
    val permissionsTotal: Int = 0,
    /** Android's VPN consent is still needed (wired CarPlay, Tesla address). */
    val vpnConsentNeeded: Boolean = false,
    val keepAlive: KeepAliveState = KeepAliveState.NOT_NEEDED,
    /** Radio readings: null when Android does not let the relay read them. */
    val bluetoothOn: Boolean? = null,
    /** This phone's Bluetooth name, the name the iPhone pairs with. */
    val bluetoothName: String? = null,
    val wifiOn: Boolean? = null,
    /** The Wi-Fi network this phone is joined to, when readable. */
    val wifiSsid: String? = null,
    val hotspotOn: Boolean? = null,
    /** The hotspot name and band the settings expect (Android does not let apps read the live ones). */
    val hotspotSsid: String? = null,
    val hotspotBand: String? = null,
    /** The saved shared Wi-Fi this phone is on now; null when it is on none of them. */
    val sharedWifiSsid: String? = null,
    val overlayNeeded: Boolean = false,
)

enum class WebPhase { OFF, STARTING, LISTENING, FAILED }
/** [code]: web_port_busy or web_failed when [phase] is FAILED. */
data class WebServerInput(val phase: WebPhase = WebPhase.OFF, val port: Int? = null, val code: String? = null,
    /** The trusted public name, when one serves the page. */
    val trustedName: String? = null, val sinceMillis: Long? = null)

/** Carlink, in the order a link comes up. OFF: the output is not chosen. */
enum class CarlinkPhase {
    OFF, UNSUPPORTED, NEEDS_PERMISSIONS, BLOCKED, SCANNING, EXCHANGING, JOINING, AUTHENTICATING, PIN,
    NEGOTIATING, PLAYING, ENDED,
}

data class CarlinkInput(
    val phase: CarlinkPhase = CarlinkPhase.OFF,
    val sinceMillis: Long? = null,
    /** Failure code (ble_timeout, wifi_join_timeout, auth_rejected, …) when the last attempt failed. */
    val code: String? = null,
    val error: String? = null,
    /** True while the link waits to retry after [code]. */
    val retrying: Boolean = false,
    val carName: String? = null,
    /** How the relay joined the car: "Wi-Fi Direct" or "Wi-Fi". */
    val joinMethod: String? = null,
    /** The car's key is in the trust store, so no PIN is needed. */
    val trusted: Boolean = false,
    val pinPending: Boolean = false,
    /** The picture size and codec the car negotiated, e.g. "1280×720 H.265". */
    val picture: String? = null,
    val trail: List<String> = emptyList(),
)

/** DLNA, Miracast and the Tesla address. WAITING_USER: the user must act (cast, choose a renderer). */
enum class OutputPhase { OFF, WAITING_USER, SEARCHING, READY, WAITING_VIDEO, STARTING, PLAYING, FAILED }
data class OutputInput(val phase: OutputPhase = OutputPhase.OFF, val name: String? = null, val code: String? = null,
    val sinceMillis: Long? = null)

data class CarInput(
    /** The first IPv4 address the car can reach this phone on (not Wi-Fi Direct, not cellular). */
    val address: String? = null,
    val web: WebServerInput = WebServerInput(),
    /** The address to open in the car's browser, trusted name first. */
    val pageUrl: String? = null,
    /** Head-unit pages connected now. */
    val pages: Int = 0,
    /** Screen size a car reported in this run, e.g. "1920×720". */
    val screen: String? = null,
    /** Screen size saved from an earlier run with this car, used when no car has reported yet. */
    val savedScreen: String? = null,
    val carlink: CarlinkInput = CarlinkInput(),
    val dlna: OutputInput = OutputInput(),
    val miracast: OutputInput = OutputInput(),
    val tesla: OutputInput = OutputInput(),
)

/** Which iPhone CarPlay will use. NOT_IPHONE: the only bonded device does not look like an iPhone. */
enum class IphonePick { NONE, PICKED, UNPAIRED, MULTIPLE, NOT_IPHONE }
enum class UsbRole { HOST, DEVICE, DISCONNECTED, UNKNOWN }

/**
 * The iPhone link, in the order DiPlay brings it up. Wireless: STARTING (MFi and network) → WAITING_IPHONE →
 * BLUETOOTH (RFCOMM) → AUTH (iAP2 identification and authentication) → WIFI_SENT (Wi-Fi details sent) →
 * JOINING (StartSession sent, waiting for the iPhone's TCP) → SESSION → VIDEO. Wired: STARTING → USB_DETECT →
 * USB_PERMISSION → USB_TRUST → USB_CONTROL → SESSION → VIDEO.
 */
enum class LinkStep {
    IDLE, STARTING, WAITING_IPHONE, BLUETOOTH, AUTH, WIFI_SENT, JOINING,
    USB_DETECT, USB_PERMISSION, USB_TRUST, USB_CONTROL, SESSION, VIDEO, ENDED,
}

/**
 * [code]: DiPlay's (hotspot_not_ready, hotspot_configuration, first_tcp_timeout, bluetooth_unusable) or the relay's
 * classification of an uncoded failure (bluetooth_off, iphone_unpaired, rfcomm_timeout, wifi_off,
 * wifi_reset_required, usb_trust_denied, iphone_locked); "unknown" otherwise.
 * [retrying]: DiPlay will try again by itself; false when it stopped.
 */
data class LinkFailure(val step: LinkStep, val code: String, val text: String, val atMillis: Long,
    val retrying: Boolean, val trail: List<String> = emptyList())

data class IphoneInput(
    val pick: IphonePick = IphonePick.NONE,
    val name: String? = null,
    val usb: UsbRole = UsbRole.UNKNOWN,
    /** Start was pressed or auto start ran, and the CarPlay host is up or coming up. */
    val started: Boolean = false,
    /** Start was pressed while a radio (Bluetooth, Wi-Fi, hotspot) was off; it continues once they are on. */
    val armed: Boolean = false,
    /** CarPlay waits for the car's screen before it starts; null when not waiting. */
    val carWaitUntilMillis: Long? = null,
    val step: LinkStep = LinkStep.IDLE,
    val stepSinceMillis: Long? = null,
    /** The latest iAP2 sub-step DiPlay reported, short and safe to show. */
    val stepDetail: String? = null,
    val failure: LinkFailure? = null,
)

data class LiveInput(
    /** The car shows the CarPlay picture: a page rendered frames, Carlink sends live frames, DLNA plays, or
     * Miracast shows. */
    val pictureOnCar: Boolean = false,
    val sessionSinceMillis: Long? = null,
    /** Short facts for the live view, already formatted: "1920×720", "60 fps", "H.265", "8.1 Mbps". */
    val chips: List<String> = emptyList(),
)
