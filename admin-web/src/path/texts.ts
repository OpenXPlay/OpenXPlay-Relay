// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-27

import type { StepId, StepAction } from './model';
export const titles: Record<StepId, string> = {
  "MODE": "path_mode",
  "IDENTITY": "path_identity",
  "PERMISSIONS": "path_permissions",
  "KEEP_ALIVE": "path_keep_alive",
  "BLUETOOTH": "path_bluetooth",
  "WIFI": "path_wifi",
  "HOTSPOT": "path_hotspot",
  "SHARED_WIFI": "path_shared_wifi",
  "CAR_NETWORK": "path_car_network",
  "WEB_SERVER": "path_web_server",
  "CAR_PAGE": "path_car_page",
  "CARLINK_FIND": "path_carlink_find",
  "CARLINK_LINK": "path_carlink_link",
  "CARLINK_PIN": "path_carlink_pin",
  "CARLINK_READY": "path_carlink_ready",
  "DLNA": "path_dlna",
  "MIRACAST": "path_miracast",
  "IPHONE_PAIRED": "path_iphone_paired",
  "USB_HOST": "path_usb_host",
  "CARPLAY_START": "path_carplay_start",
  "IPHONE_BLUETOOTH": "path_iphone_bluetooth",
  "IPHONE_AUTH": "path_iphone_auth",
  "IPHONE_WIFI": "path_iphone_wifi",
  "USB_IPHONE": "path_usb_iphone",
  "USB_TRUST": "path_usb_trust",
  "USB_CONTROL": "path_usb_control",
  "SESSION": "path_session",
  "TESLA": "path_tesla",
  "PICTURE": "path_picture"
};
export const explanations: Partial<Record<StepId, string>> = {
  "BLUETOOTH": "path_explain_bluetooth",
  "HOTSPOT": "path_explain_hotspot",
  "CAR_NETWORK": "path_explain_car_network",
  "CAR_PAGE": "path_explain_car_page",
  "CARPLAY_START": "path_explain_carplay_start"
};
export const actionLabels: Record<StepAction, string> = {
  "FIX_SETTINGS": "path_action_fix_settings",
  "IMPORT_IDENTITY": "path_action_import_identity",
  "GRANT_PERMISSIONS": "path_action_grant_permissions",
  "KEEP_ALIVE_SETTINGS": "path_action_keep_alive_settings",
  "TURN_ON_BLUETOOTH": "path_action_turn_on",
  "OPEN_WIFI": "path_action_turn_on",
  "OPEN_HOTSPOT": "path_action_open_hotspot",
  "CHOOSE_SHARED_WIFI": "path_action_choose",
  "CHOOSE_RENDERER": "path_action_choose",
  "CHOOSE_IPHONE": "path_action_choose",
  "COPY_PAGE_ADDRESS": "path_action_copy_page_address",
  "RECONNECT_CARLINK": "path_action_retry",
  "RETRY": "path_action_retry",
  "ENTER_PIN": "path_action_enter_pin",
  "OPEN_CAST": "path_action_open_cast",
  "PAIR_IPHONE": "path_action_pair_iphone",
  "START_NOW": "path_action_start_now"
};
const failures: Record<string, string> = {
  "bluetooth_off": "path_explain_bluetooth_off",
  "iphone_unpaired": "path_explain_iphone_unpaired",
  "rfcomm_timeout": "path_explain_rfcomm_timeout",
  "wifi_off": "path_explain_wifi_off",
  "wifi_reset_required": "path_explain_wifi_reset_required",
  "usb_trust_denied": "path_explain_usb_trust_denied",
  "iphone_locked": "path_explain_iphone_locked",
  "usb_role": "path_explain_usb_role",
  "carlink_blocked": "path_explain_carlink_blocked",
  "web_port_busy": "path_explain_web_port_busy",
  "auth_rejected": "path_explain_auth_rejected",
  "auth_timeout": "path_explain_auth_rejected",
  "pin_cancelled": "path_explain_auth_rejected",
  "peer_key_changed": "path_explain_peer_key_changed",
  "projection_timeout": "path_explain_projection_timeout",
  "heartbeat_timeout": "path_explain_heartbeat_timeout",
  "wifi_link_lost": "path_explain_heartbeat_timeout",
  "hotspot_not_ready": "error_explain_hotspot_not_ready",
  "hotspot_configuration": "error_explain_hotspot_configuration",
  "first_tcp_timeout": "error_explain_first_tcp_timeout",
  "bluetooth_unusable": "error_explain_bluetooth_unusable",
  "ble_unavailable": "error_explain_ble_unavailable",
  "ble_timeout": "error_explain_ble_timeout",
  "ble_scan_failed": "error_explain_ble_scan_failed",
  "gatt_connect_failed": "error_explain_gatt_connect_failed",
  "gatt_services_failed": "error_explain_gatt_services_failed",
  "ble_discovery_failed": "error_explain_gatt_services_failed",
  "ble_service_missing": "error_explain_ble_service_missing",
  "ble_subscribe_failed": "error_explain_ble_subscribe_failed",
  "ble_cccd_failed": "error_explain_ble_cccd_failed",
  "ble_cccd_timeout": "error_explain_ble_cccd_timeout",
  "ble_write_failed": "error_explain_ble_write_failed",
  "ble_request_rejected": "error_explain_ble_request_rejected",
  "ble_reply_invalid": "error_explain_ble_reply_invalid",
  "ble_verify_timeout": "error_explain_ble_verify_timeout",
  "ble_exchange_timeout": "error_explain_ble_exchange_timeout",
  "ble_exchange_failed": "error_explain_ble_exchange_failed",
  "renderer_not_found": "dlna_error_not_found",
  "renderer_unreachable": "dlna_error_unreachable",
  "renderer_no_avtransport": "dlna_error_no_avtransport",
  "renderer_rejected": "dlna_error_rejected",
  "renderer_stopped": "dlna_error_stopped",
  "stream_server_failed": "dlna_error_server",
  "unsupported_video": "dlna_error_video",
  "overlay_denied": "miracast_allow_overlay",
  "window_failed": "miracast_error_window",
  "decoder_failed": "miracast_error_decoder",
  "display_lost": "miracast_error_display"
};
export function failureExplanation(code: string | null): string | null { return code ? failures[code] ?? (code.startsWith('wifi_join_') ? 'path_explain_wifi_join' : null) : null; }

const linuxFailures: Record<string, string> = {
  carlink_not_ready: 'path_explain_carlink_not_ready',
  iphone_unplugged: 'path_explain_iphone_unplugged',
  usb_interface_busy: 'path_explain_usb_interface_busy',
};
export function linuxFailureExplanation(code: string): string | null { return linuxFailures[code] ?? failureExplanation(code); }
export function failureHeadline(code: string): string {
  if (code === 'carlink_not_ready') return 'path_failed_carlink_not_ready';
  if (code === 'iphone_unplugged') return 'path_failed_iphone_unplugged';
  if (code === 'usb_interface_busy') return 'path_failed_usb_interface_busy';
  if (code === 'first_tcp_timeout') return 'path_iphone_did_not_join';
  if (code.startsWith('ble_') || code.startsWith('gatt_') || ['carlink_blocked', 'projection_timeout', 'auth_rejected', 'auth_timeout', 'peer_key_changed', 'pin_cancelled'].includes(code)) return 'path_link_failed';
  return 'path_failed_unknown';
}
