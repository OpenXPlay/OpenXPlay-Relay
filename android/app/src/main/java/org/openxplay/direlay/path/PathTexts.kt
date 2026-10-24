// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-24
package org.openxplay.direlay.path

import org.openxplay.direlay.R

fun StepId.titleResource(): Int = when (this) {
    StepId.MODE -> R.string.path_mode
    StepId.IDENTITY -> R.string.path_identity
    StepId.PERMISSIONS -> R.string.path_permissions
    StepId.KEEP_ALIVE -> R.string.path_keep_alive
    StepId.BLUETOOTH -> R.string.path_bluetooth
    StepId.WIFI -> R.string.path_wifi
    StepId.HOTSPOT -> R.string.path_hotspot
    StepId.SHARED_WIFI -> R.string.path_shared_wifi
    StepId.CAR_NETWORK -> R.string.path_car_network
    StepId.WEB_SERVER -> R.string.path_web_server
    StepId.CAR_PAGE -> R.string.path_car_page
    StepId.CARLINK_FIND -> R.string.path_carlink_find
    StepId.CARLINK_LINK -> R.string.path_carlink_link
    StepId.CARLINK_PIN -> R.string.path_carlink_pin
    StepId.CARLINK_READY -> R.string.path_carlink_ready
    StepId.DLNA -> R.string.path_dlna
    StepId.MIRACAST -> R.string.path_miracast
    StepId.IPHONE_PAIRED -> R.string.path_iphone_paired
    StepId.USB_HOST -> R.string.path_usb_host
    StepId.CARPLAY_START -> R.string.path_carplay_start
    StepId.IPHONE_BLUETOOTH -> R.string.path_iphone_bluetooth
    StepId.IPHONE_AUTH -> R.string.path_iphone_auth
    StepId.IPHONE_WIFI -> R.string.path_iphone_wifi
    StepId.USB_IPHONE -> R.string.path_usb_iphone
    StepId.USB_TRUST -> R.string.path_usb_trust
    StepId.USB_CONTROL -> R.string.path_usb_control
    StepId.SESSION -> R.string.path_session
    StepId.TESLA -> R.string.path_tesla
    StepId.PICTURE -> R.string.path_picture
}

fun PathSection.titleResource(): Int = when (this) {
    PathSection.RELAY -> R.string.path_section_relay
    PathSection.CAR -> R.string.path_section_car
    PathSection.IPHONE -> R.string.path_section_iphone
    PathSection.PICTURE -> R.string.path_section_picture
}

fun pathFailureExplanation(code: String?): Int? = when (code) {
    "bluetooth_off" -> R.string.path_explain_bluetooth_off
    "iphone_unpaired" -> R.string.path_explain_iphone_unpaired
    "rfcomm_timeout" -> R.string.path_explain_rfcomm_timeout
    "wifi_off" -> R.string.path_explain_wifi_off
    "wifi_reset_required" -> R.string.path_explain_wifi_reset_required
    "usb_trust_denied" -> R.string.path_explain_usb_trust_denied
    "iphone_locked" -> R.string.path_explain_iphone_locked
    "usb_role" -> R.string.path_explain_usb_role
    "carlink_blocked" -> R.string.path_explain_carlink_blocked
    "web_port_busy" -> R.string.path_explain_web_port_busy
    "auth_rejected", "auth_timeout", "pin_cancelled" -> R.string.path_explain_auth_rejected
    "peer_key_changed" -> R.string.path_explain_peer_key_changed
    "projection_timeout" -> R.string.path_explain_projection_timeout
    "heartbeat_timeout", "wifi_link_lost" -> R.string.path_explain_heartbeat_timeout
    "hotspot_not_ready" -> R.string.error_explain_hotspot_not_ready
    "hotspot_configuration" -> R.string.error_explain_hotspot_configuration
    "first_tcp_timeout" -> R.string.error_explain_first_tcp_timeout
    "bluetooth_unusable" -> R.string.error_explain_bluetooth_unusable
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
    "renderer_not_found" -> R.string.dlna_error_not_found
    "renderer_unreachable" -> R.string.dlna_error_unreachable
    "renderer_no_avtransport" -> R.string.dlna_error_no_avtransport
    "renderer_rejected" -> R.string.dlna_error_rejected
    "renderer_stopped" -> R.string.dlna_error_stopped
    "stream_server_failed" -> R.string.dlna_error_server
    "unsupported_video" -> R.string.dlna_error_video
    "overlay_denied" -> R.string.miracast_allow_overlay
    "window_failed" -> R.string.miracast_error_window
    "decoder_failed" -> R.string.miracast_error_decoder
    "display_lost" -> R.string.miracast_error_display
    else -> if (code?.startsWith("wifi_join_") == true) R.string.path_explain_wifi_join else null
}

internal fun StepId.explanationResource(): Int? = when (this) {
    StepId.BLUETOOTH -> R.string.path_explain_bluetooth
    StepId.HOTSPOT -> R.string.path_explain_hotspot
    StepId.CAR_NETWORK -> R.string.path_explain_car_network
    StepId.CAR_PAGE -> R.string.path_explain_car_page
    StepId.CARPLAY_START -> R.string.path_explain_carplay_start
    else -> null
}

internal fun StepAction.labelResource(): Int = when (this) {
    StepAction.FIX_SETTINGS -> R.string.path_action_fix_settings
    StepAction.IMPORT_IDENTITY -> R.string.path_action_import_identity
    StepAction.GRANT_PERMISSIONS -> R.string.path_action_grant_permissions
    StepAction.KEEP_ALIVE_SETTINGS -> R.string.path_action_keep_alive_settings
    StepAction.TURN_ON_BLUETOOTH, StepAction.OPEN_WIFI -> R.string.path_action_turn_on
    StepAction.OPEN_HOTSPOT -> R.string.path_action_open_hotspot
    StepAction.CHOOSE_SHARED_WIFI, StepAction.CHOOSE_RENDERER, StepAction.CHOOSE_IPHONE -> R.string.path_action_choose
    StepAction.COPY_PAGE_ADDRESS -> R.string.path_action_copy_page_address
    StepAction.RECONNECT_CARLINK, StepAction.RETRY -> R.string.path_action_retry
    StepAction.ENTER_PIN -> R.string.path_action_enter_pin
    StepAction.OPEN_CAST -> R.string.path_action_open_cast
    StepAction.PAIR_IPHONE -> R.string.path_action_pair_iphone
    StepAction.START_NOW -> R.string.path_action_start_now
}
