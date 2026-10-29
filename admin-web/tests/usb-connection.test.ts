// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-22

import { expect, it } from 'vitest';
import { needsUsbConfirmation, usbTransportHint } from '../src/usb-connection';
import { translate } from '../src/i18n';

it('confirms a change to USB on Android', () => {
  const platform = 'android';
  expect(needsUsbConfirmation(platform, 'wireless', 'usb')).toBe(true);
  expect(needsUsbConfirmation(platform, 'usb', 'usb')).toBe(false);
  for (const before of ['wireless', 'usb']) expect(needsUsbConfirmation(platform, before, 'wireless')).toBe(false);
  expect(usbTransportHint(platform, 'usb')).toBe(`hint.option.transport.usb.${platform}`);
  expect(usbTransportHint(platform, 'wireless')).toBeUndefined();
});
it('does not confirm USB when the platform is unknown', () => {
  expect(needsUsbConfirmation(undefined, 'wireless', 'usb')).toBe(false);
  expect(usbTransportHint(undefined, 'usb')).toBeUndefined();
});
it('keeps the Android warning and three confirmation paragraphs in both languages', () => {
  for (const language of ['en', 'zh-CN'] as const) {
    const hint = usbTransportHint('android', 'usb')!, confirmation = 'confirm.transport.usb.android';
    expect(translate(language, hint)).not.toBe(hint);
    const text = translate(language, confirmation);
    expect(text).not.toBe(confirmation);
    expect(text.split('\n\n')).toHaveLength(3);
    expect(text).not.toContain('\\n');
  }
});
it('gives Linux cable advice without the Android battery warning in both languages', () => {
  for (const language of ['en', 'zh-CN'] as const) {
    const hint = translate(language, usbTransportHint('linux', 'usb')!);
    for (const cable of ['USB-A', 'USB-C', 'Lightning']) expect(hint).toContain(cable);
    expect(needsUsbConfirmation('linux', 'wireless', 'usb')).toBe(false);
    expect(hint).not.toMatch(/battery|warm|电池|发热/);
  }
  expect(translate('en', usbTransportHint('linux', 'usb')!)).toContain('Charge-only cables do not work');
});
