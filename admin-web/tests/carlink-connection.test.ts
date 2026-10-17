// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-16

import { expect, it } from 'vitest';
import { connectionConstraint, constraintKey, needsHotspotConfirmation, networkOptionState } from '../src/carlink-connection';
import { translate } from '../src/i18n';

it('shows the same explanation at connection and output choices in both languages', () => {
  for (const language of ['en', 'zh-CN'] as const) {
    const t = (key: string) => translate(language, key);
    expect(t(constraintKey)).not.toBe(constraintKey);
    expect(connectionConstraint('android', t)).toContain(t(constraintKey));
    expect(t('constraint.carlinkWifiDirectConfirm')).not.toBe('constraint.carlinkWifiDirectConfirm');
    expect(connectionConstraint('linux', t)).toBe('');
  }
});
it('disables Wi-Fi Direct for either Carlink output only on Android', () => {
  for (const output of ['web', 'carlink', 'both']) for (const platform of ['android', 'linux']) {
    expect(networkOptionState(platform, { output }, 'wifi-direct')).toBe(platform === 'android' && output !== 'web' ? 'disabled' : '');
    for (const mode of ['phone-hotspot', 'shared-wifi', 'device-hotspot']) expect(networkOptionState(platform, { output }, mode)).toBe('');
  }
});
it('requires confirmation only when enabling Carlink changes the wireless mode', () => {
  for (const output of ['carlink', 'both']) {
    expect(needsHotspotConfirmation('android', { output: 'web', networkMode: 'wifi-direct', transport: 'wireless' }, output)).toBe(true);
    expect(needsHotspotConfirmation('android', { output: 'web', networkMode: 'wifi-direct', transport: 'usb' }, output)).toBe(false);
    expect(needsHotspotConfirmation('linux', { output: 'web', networkMode: 'wifi-direct' }, output)).toBe(false);
    for (const networkMode of ['phone-hotspot', 'shared-wifi']) expect(needsHotspotConfirmation('android', { output: 'web', networkMode }, output)).toBe(false);
  }
});
