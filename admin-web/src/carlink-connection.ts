// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-16

import type { Values, Value } from './schema';
export const constraintKey = 'constraint.carlinkWifiDirect';
export function carlinkEnabled(values: Values): boolean { return ['carlink', 'both'].includes(String(values.output)); }
export function wifiDirectDisabled(platform: string | undefined, values: Values, mode: Value): boolean {
  return platform === 'android' && carlinkEnabled(values) && mode === 'wifi-direct';
}
export function needsHotspotConfirmation(platform: string | undefined, values: Values, output: Value): boolean {
  return platform === 'android' && !carlinkEnabled(values) && ['carlink', 'both'].includes(String(output)) && values.transport !== 'usb' && values.networkMode === 'wifi-direct';
}
export function connectionConstraint(platform: string | undefined, t: (key: string) => string): string {
  return platform === 'android' ? `<p class="muted">${t(constraintKey)}</p>` : '';
}
export function networkOptionState(platform: string | undefined, values: Values, mode: Value): string {
  return wifiDirectDisabled(platform, values, mode) ? 'disabled' : '';
}
