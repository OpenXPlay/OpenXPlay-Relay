// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-24
package org.openxplay.direlay.path

/**
 * The parts of the connection, in the order they must come up: this phone's own preparation, then the car,
 * then the iPhone (so CarPlay starts with the car's real screen), then the picture on the car.
 */
enum class PathSection(val order: Int, val code: String) {
    RELAY(1, "R"), CAR(2, "C"), IPHONE(3, "I"), PICTURE(4, "P")
}

/**
 * OK: checked and true. WORKING: the relay is doing it now. WAITING: waiting for something outside this phone
 * (the car, the iPhone). ACTION: the user must do something on this phone. FAILED: tried and failed.
 * BLOCKED: an earlier step has to pass first.
 */
enum class StepState(val glyph: Char) {
    OK('✓'), WORKING('~'), WAITING('~'), ACTION('!'), FAILED('✗'), BLOCKED('·')
}

/** Every step there is; a path holds only the ones the chosen setup needs, in this order. */
enum class StepId(val section: PathSection, val code: String) {
    MODE(PathSection.RELAY, "MODE"),
    IDENTITY(PathSection.RELAY, "ID"),
    PERMISSIONS(PathSection.RELAY, "PERM"),
    KEEP_ALIVE(PathSection.RELAY, "BG"),
    BLUETOOTH(PathSection.RELAY, "BT"),
    WIFI(PathSection.RELAY, "WIFI"),
    HOTSPOT(PathSection.RELAY, "HS"),
    SHARED_WIFI(PathSection.RELAY, "LAN"),

    CAR_NETWORK(PathSection.CAR, "NET"),
    WEB_SERVER(PathSection.CAR, "WEB"),
    CAR_PAGE(PathSection.CAR, "PAGE"),
    CARLINK_FIND(PathSection.CAR, "BLE"),
    CARLINK_LINK(PathSection.CAR, "LINK"),
    CARLINK_PIN(PathSection.CAR, "PIN"),
    CARLINK_READY(PathSection.CAR, "CL"),
    DLNA(PathSection.CAR, "DLNA"),
    MIRACAST(PathSection.CAR, "MC"),

    IPHONE_PAIRED(PathSection.IPHONE, "PAIR"),
    USB_HOST(PathSection.IPHONE, "USB"),
    CARPLAY_START(PathSection.IPHONE, "CP"),
    IPHONE_BLUETOOTH(PathSection.IPHONE, "RF"),
    IPHONE_AUTH(PathSection.IPHONE, "IAP"),
    IPHONE_WIFI(PathSection.IPHONE, "JOIN"),
    USB_IPHONE(PathSection.IPHONE, "DEV"),
    USB_TRUST(PathSection.IPHONE, "TRUST"),
    USB_CONTROL(PathSection.IPHONE, "CTRL"),
    SESSION(PathSection.IPHONE, "SES"),

    TESLA(PathSection.PICTURE, "TSLA"),
    PICTURE(PathSection.PICTURE, "LIVE"),
}

/** What a step's button does; the host maps each to an Android action. */
enum class StepAction {
    FIX_SETTINGS, IMPORT_IDENTITY, GRANT_PERMISSIONS, KEEP_ALIVE_SETTINGS,
    TURN_ON_BLUETOOTH, OPEN_WIFI, OPEN_HOTSPOT, CHOOSE_SHARED_WIFI,
    COPY_PAGE_ADDRESS, RECONNECT_CARLINK, ENTER_PIN, CHOOSE_RENDERER, OPEN_CAST,
    PAIR_IPHONE, CHOOSE_IPHONE, START_NOW, RETRY,
}

/** Text that is either a string resource with arguments or a value shown as it is (names, addresses, sizes). */
data class StepText(val res: Int? = null, val args: List<String> = emptyList(), val raw: String? = null) {
    companion object {
        fun of(res: Int, vararg args: String) = StepText(res, args.toList())
        fun raw(value: String) = StepText(raw = value)
    }
}

/**
 * One step of the path. [title] is two to four words; [detail] one short line saying what is happening or what
 * to do; [evidence] the value that was checked (shown verbatim, monospace); [code] the machine reason for a
 * failure or wait, also printed on the diagnostic line; [explanation] the longer text of the step sheet;
 * [sinceMillis] when the current state began, for elapsed timers.
 */
data class PathStep(
    val id: StepId,
    val state: StepState,
    val title: Int,
    val detail: StepText? = null,
    val evidence: StepText? = null,
    val code: String? = null,
    val action: StepAction? = null,
    val actionLabel: Int? = null,
    val sinceMillis: Long? = null,
    val explanation: Int? = null,
    val trail: List<String> = emptyList(),
    val carWaitUntilMillis: Long? = null,
)

data class PathSectionView(val section: PathSection, val steps: List<PathStep>) {
    val done get() = steps.count { it.state == StepState.OK }
    val total get() = steps.size
    /** The section's own state: the most urgent state among its steps. */
    val state: StepState get() = when {
        steps.isEmpty() || steps.all { it.state == StepState.OK } -> StepState.OK
        steps.any { it.state == StepState.FAILED } -> StepState.FAILED
        steps.any { it.state == StepState.ACTION } -> StepState.ACTION
        steps.any { it.state == StepState.WORKING } -> StepState.WORKING
        steps.any { it.state == StepState.WAITING } -> StepState.WAITING
        else -> StepState.BLOCKED
    }
}

/** IDLE: stopped and not ready; READY: stopped, ready to start; LIVE: the picture is on the car. */
enum class PathTone { IDLE, READY, PROGRESS, ACTION, ERROR, LIVE }

/** Names on the path map: the iPhone, this phone, the car (car name, page address or renderer). */
data class PathNodes(val iphone: String? = null, val relay: String? = null, val car: String? = null)

data class LiveSummary(val sinceMillis: Long?, val chips: List<String>)

data class ConnectionPath(
    val sections: List<PathSectionView> = emptyList(),
    /** The first step, in path order, that is not OK: what the user should look at now. Null when live. */
    val focus: PathStep? = null,
    val tone: PathTone = PathTone.IDLE,
    /** Start can be pressed: nothing on this phone needs the user first (radios may still be armed for). */
    val canStart: Boolean = false,
    /** A CarPlay run was started and has not been stopped. */
    val running: Boolean = false,
    val live: LiveSummary? = null,
    val nodes: PathNodes = PathNodes(),
    /** One line for screenshots and logs, e.g. "R MODE✓ ID✓ BT✓ HS! | C NET· … | E hotspot_configuration 14:31:58". */
    val diagnostic: String = "",
    val version: String = "",
) {
    fun section(section: PathSection) = sections.firstOrNull { it.section == section }
    val steps get() = sections.flatMap { it.steps }
}
