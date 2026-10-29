// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-22

import type { Value } from './schema';

export function needsUsbConfirmation(platform: string | undefined, before: Value, after: Value): boolean {
  return platform === 'android' && before !== 'usb' && after === 'usb';
}
export function usbTransportHint(platform: string | undefined, transport: Value): string | undefined {
  return transport === 'usb' && (platform === 'android' || platform === 'linux') ? `hint.option.transport.usb.${platform}` : undefined;
}
