// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-22

import { expect, it } from 'vitest';
import { needsUsbConfirmation, usbTransportHint } from '../src/usb-connection';
import { translate } from '../src/i18n';

it('confirms a change to USB on Android', () => {
  expect(needsUsbConfirmation('android', 'wireless', 'usb')).toBe(true);
});
it('does not confirm USB on Linux or when the platform is unknown', () => {
  expect(needsUsbConfirmation('linux', 'wireless', 'usb')).toBe(false);
  expect(needsUsbConfirmation(undefined, 'wireless', 'usb')).toBe(false);
});
it('does not confirm USB when it is already selected', () => {
  expect(needsUsbConfirmation('android', 'usb', 'usb')).toBe(false);
});
it('does not confirm switching to wireless', () => {
  for (const before of ['wireless', 'usb'])
    expect(needsUsbConfirmation('android', before, 'wireless')).toBe(false);
});
it('adds the USB option hint only on Android', () => {
  expect(usbTransportHint('android', 'usb')).toBe('hint.option.transport.usb.android');
  expect(usbTransportHint('android', 'wireless')).toBeUndefined();
  expect(usbTransportHint('linux', 'usb')).toBeUndefined();
  expect(usbTransportHint(undefined, 'usb')).toBeUndefined();
});
it('translates the Android warning and confirmation with separate paragraphs in both languages', () => {
  const hint = usbTransportHint('android', 'usb')!;
  const confirmation = 'confirm.transport.usb.android';
  for (const language of ['en', 'zh-CN'] as const) {
    expect(translate(language, hint)).not.toBe(hint);
    const text = translate(language, confirmation);
    expect(text).not.toBe(confirmation);
    expect(text.split('\n\n')).toHaveLength(3);
    expect(text).not.toContain('\\n');
  }
});
