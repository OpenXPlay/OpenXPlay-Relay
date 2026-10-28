// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-22

import { expect, it } from 'vitest';
import { needsUsbConfirmation, usbTransportHint, usbConfirmationKey } from '../src/usb-connection';
import { translate } from '../src/i18n';

it.each(['android', 'linux'])('confirms a change to USB on %s', platform => {
  expect(needsUsbConfirmation(platform, 'wireless', 'usb')).toBe(true);
  expect(needsUsbConfirmation(platform, 'usb', 'usb')).toBe(false);
  for (const before of ['wireless', 'usb']) expect(needsUsbConfirmation(platform, before, 'wireless')).toBe(false);
  expect(usbTransportHint(platform, 'usb')).toBe(`hint.option.transport.usb.${platform}`);
  expect(usbConfirmationKey(platform)).toBe(`confirm.transport.usb.${platform}`);
  expect(usbTransportHint(platform, 'wireless')).toBeUndefined();
});
it('does not confirm USB when the platform is unknown', () => {
  expect(needsUsbConfirmation(undefined, 'wireless', 'usb')).toBe(false);
  expect(usbTransportHint(undefined, 'usb')).toBeUndefined();
});
it('keeps the Android warning and three confirmation paragraphs in both languages', () => {
  for (const language of ['en', 'zh-CN'] as const) {
    const hint = usbTransportHint('android', 'usb')!, confirmation = usbConfirmationKey('android');
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
    const confirmation = translate(language, usbConfirmationKey('linux'));
    for (const cable of ['USB-A', 'USB-C', 'Lightning']) expect(hint).toContain(cable);
    expect(confirmation.split('\n\n')).toHaveLength(2);
    expect(confirmation).toContain(hint);
    expect(confirmation).not.toMatch(/battery|warm|电池|发热/);
  }
  expect(translate('en', usbConfirmationKey('linux'))).toContain('Charge-only cables do not work');
});
