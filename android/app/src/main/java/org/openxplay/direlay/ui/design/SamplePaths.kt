// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-24
package org.openxplay.direlay.ui.design

import org.openxplay.direlay.R
import org.openxplay.direlay.path.*

data class DesignSample(val title: Int, val path: ConnectionPath)

/** Hand-built presentation fixtures, independent of the runtime and presenter resources. */
fun samplePaths(nowMillis: Long, pagesLabel: String = "1"): List<DesignSample> {
    val relay = listOf(StepId.MODE, StepId.IDENTITY, StepId.PERMISSIONS, StepId.BLUETOOTH, StepId.HOTSPOT)
    val car = listOf(StepId.CAR_NETWORK, StepId.WEB_SERVER, StepId.CAR_PAGE)
    val iphone = listOf(StepId.IPHONE_PAIRED, StepId.CARPLAY_START, StepId.IPHONE_BLUETOOTH, StepId.IPHONE_AUTH, StepId.IPHONE_WIFI, StepId.SESSION)
    val ids = relay + car + iphone + StepId.PICTURE
    val evidence = mapOf(StepId.MODE to "Wi-Fi · HTTPS", StepId.IDENTITY to "OpenXPlay", StepId.PERMISSIONS to "5/5",
        StepId.BLUETOOTH to "Redmi 14C", StepId.HOTSPOT to "OpenXPlay-1A2B · 5 GHz", StepId.CAR_NETWORK to "192.168.43.1",
        StepId.WEB_SERVER to ":8443 HTTPS", StepId.CAR_PAGE to "https://192.168.43.1:8443", StepId.IPHONE_PAIRED to "iPhone 15",
        StepId.SESSION to "1920×720 · 60 fps", StepId.PICTURE to "H.264 · 8.2 Mb/s")
    fun build(title: Int, tone: PathTone, running: Boolean, canStart: Boolean, focus: StepId?,
        special: PathStep? = null, completed: Set<StepId> = emptySet(), live: Boolean = false, pin: Boolean = false): DesignSample {
        val selectedIds = if (pin) relay + listOf(StepId.CARLINK_FIND, StepId.CARLINK_LINK, StepId.CARLINK_PIN, StepId.CARLINK_READY) + iphone + StepId.PICTURE else ids
        val steps = selectedIds.map { id ->
            if (id == special?.id) special else PathStep(id, if (id in completed || live) StepState.OK else StepState.BLOCKED,
                sampleStepTitle(id), evidence = evidence[id]?.takeIf { id in completed || live }?.let(StepText::raw),
                detail = when {
                    id in completed || live -> null
                    id == StepId.CARPLAY_START && running && focus == StepId.HOTSPOT -> StepText.of(R.string.design_armed_detail)
                    id == StepId.CARPLAY_START && !running && canStart -> StepText.of(R.string.design_tap_connect)
                    id == StepId.PICTURE -> StepText.of(R.string.design_after_carplay)
                    else -> StepText.of(R.string.design_after_step)
                })
        }
        val sections = PathSection.entries.map { section -> PathSectionView(section, steps.filter { it.id.section == section }) }
        val line = sections.joinToString(" | ") { section -> section.section.code + " " + section.steps.joinToString(" ") { it.id.code + it.state.glyph } }
        return DesignSample(title, ConnectionPath(sections, steps.firstOrNull { it.id == focus }, tone, canStart, running,
            if (live) LiveSummary(nowMillis - 751_000, listOf("1920×720", "60 fps", "H.264", "8.2 Mb/s", pagesLabel)) else null,
            PathNodes("iPhone 15", "Redmi 14C", if (pin) "Carlink" else "1920×720"),
            line + (special?.takeIf { it.state == StepState.FAILED }?.code?.let { " | E $it 14:31:58" } ?: "") + " | 0.1.6 14:32:05"))
    }
    val prepared = (relay + StepId.IPHONE_PAIRED).toSet()
    val started = (relay + car + iphone.take(2)).toSet()
    return listOf(
        build(R.string.design_sample_idle, PathTone.ACTION, false, false, StepId.IDENTITY,
            PathStep(StepId.IDENTITY, StepState.ACTION, sampleStepTitle(StepId.IDENTITY), StepText.of(R.string.design_import_identity),
                action = StepAction.IMPORT_IDENTITY, actionLabel = R.string.design_import), setOf(StepId.MODE)),
        build(R.string.design_sample_ready, PathTone.READY, false, true, StepId.CAR_PAGE,
            PathStep(StepId.CAR_PAGE, StepState.WAITING, sampleStepTitle(StepId.CAR_PAGE), StepText.of(R.string.design_open_car),
                StepText.raw("https://192.168.43.1:8443"), action = StepAction.COPY_PAGE_ADDRESS, actionLabel = R.string.ui_copy),
            prepared + car.take(2)),
        build(R.string.design_sample_armed, PathTone.ACTION, true, false, StepId.HOTSPOT,
            PathStep(StepId.HOTSPOT, StepState.ACTION, sampleStepTitle(StepId.HOTSPOT), StepText.of(R.string.design_turn_on_hotspot, "OpenXPlay-1A2B"),
                StepText.raw("OpenXPlay-1A2B · 5 GHz"), action = StepAction.OPEN_HOTSPOT, actionLabel = R.string.design_open,
                explanation = R.string.design_hotspot_explanation), prepared - StepId.HOTSPOT),
        build(R.string.design_sample_car_wait, PathTone.PROGRESS, true, false, StepId.CARPLAY_START,
            PathStep(StepId.CARPLAY_START, StepState.WAITING, sampleStepTitle(StepId.CARPLAY_START), StepText.of(R.string.design_car_countdown, "12"),
                action = StepAction.START_NOW, actionLabel = R.string.design_start_now, sinceMillis = nowMillis - 8000), prepared + car.take(2)),
        build(R.string.design_sample_connecting, PathTone.PROGRESS, true, false, StepId.IPHONE_BLUETOOTH,
            PathStep(StepId.IPHONE_BLUETOOTH, StepState.WORKING, sampleStepTitle(StepId.IPHONE_BLUETOOTH), StepText.of(R.string.design_connecting_iphone, "iPhone 15"),
                sinceMillis = nowMillis - 12_000), started),
        build(R.string.design_sample_failed, PathTone.ERROR, true, false, StepId.IPHONE_WIFI,
            PathStep(StepId.IPHONE_WIFI, StepState.FAILED, sampleStepTitle(StepId.IPHONE_WIFI), StepText.of(R.string.design_join_failed),
                StepText.raw("OpenXPlay-1A2B"), "first_tcp_timeout", StepAction.RETRY, R.string.design_retry, nowMillis - 30_000,
                R.string.design_join_explanation, listOf("14:31:28  IPHONE_WIFI WORKING", "14:31:58  first_tcp_timeout")), started + iphone.take(4)),
        build(R.string.design_sample_retrying, PathTone.PROGRESS, true, false, StepId.IPHONE_BLUETOOTH,
            PathStep(StepId.IPHONE_BLUETOOTH, StepState.WORKING, sampleStepTitle(StepId.IPHONE_BLUETOOTH), StepText.of(R.string.design_retrying, "rfcomm_timeout"),
                code = "rfcomm_timeout", sinceMillis = nowMillis - 3000), started),
        build(R.string.design_sample_pin, PathTone.ACTION, true, false, StepId.CARLINK_PIN,
            PathStep(StepId.CARLINK_PIN, StepState.ACTION, sampleStepTitle(StepId.CARLINK_PIN), StepText.of(R.string.design_enter_pin),
                action = StepAction.ENTER_PIN, actionLabel = R.string.design_enter), prepared + setOf(StepId.CARLINK_FIND, StepId.CARLINK_LINK), pin = true),
        build(R.string.design_sample_live, PathTone.LIVE, true, false, null, live = true),
    )
}

fun sampleStepTitle(id: StepId): Int = when (id) {
    StepId.MODE -> R.string.design_step_mode
    StepId.IDENTITY -> R.string.design_step_identity
    StepId.PERMISSIONS -> R.string.design_step_permissions
    StepId.KEEP_ALIVE -> R.string.design_step_keep_alive
    StepId.BLUETOOTH -> R.string.design_step_bluetooth
    StepId.WIFI -> R.string.design_step_wifi
    StepId.HOTSPOT -> R.string.design_step_hotspot
    StepId.SHARED_WIFI -> R.string.design_step_shared_wifi
    StepId.CAR_NETWORK -> R.string.design_step_car_network
    StepId.WEB_SERVER -> R.string.design_step_web_server
    StepId.CAR_PAGE -> R.string.design_step_car_page
    StepId.CARLINK_FIND -> R.string.design_step_carlink_find
    StepId.CARLINK_LINK -> R.string.design_step_carlink_link
    StepId.CARLINK_PIN -> R.string.design_step_carlink_pin
    StepId.CARLINK_READY -> R.string.design_step_carlink_ready
    StepId.DLNA -> R.string.design_step_dlna
    StepId.MIRACAST -> R.string.design_step_miracast
    StepId.IPHONE_PAIRED -> R.string.design_step_iphone_paired
    StepId.USB_HOST -> R.string.design_step_usb_host
    StepId.CARPLAY_START -> R.string.design_step_carplay_start
    StepId.IPHONE_BLUETOOTH -> R.string.design_step_iphone_bluetooth
    StepId.IPHONE_AUTH -> R.string.design_step_iphone_auth
    StepId.IPHONE_WIFI -> R.string.design_step_iphone_wifi
    StepId.USB_IPHONE -> R.string.design_step_usb_iphone
    StepId.USB_TRUST -> R.string.design_step_usb_trust
    StepId.USB_CONTROL -> R.string.design_step_usb_control
    StepId.SESSION -> R.string.design_step_session
    StepId.TESLA -> R.string.design_step_tesla
    StepId.PICTURE -> R.string.design_step_picture
}
