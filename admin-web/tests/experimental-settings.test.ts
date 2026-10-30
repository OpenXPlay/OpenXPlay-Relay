// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-30

import { expect, it } from 'vitest';
import { mockSettings } from '../dev-server';
import { fieldEnabled, pageFields } from '../src/settings-tree';
import { translate } from '../src/i18n';

const choices = ['experimentalIapChannelWhenOffered', 'experimentalReleaseBluetoothOnRequest', 'experimentalAwaitIphoneReconnect', 'experimentalRadioConflictPrompt', 'experimentalStallWatchdog'];

it('renders the five Linux choices on Experimental and parks them under the master', () => {
  const settings = mockSettings(true);
  const fields = pageFields(settings, 'settings/experimental');
  for (const key of choices) {
    const field = fields.find(f => f.key === key)!;
    expect(field.type, key).toBe('bool');
    expect(field.labelKey).toBe(`field.${key}`);
    expect(field.hintKey).toBe(`hint.field.${key}`);
    expect(settings.values[key]).toBe(true);
    expect(fieldEnabled(key, { experimentalActive: true, [key]: false })).toBe(true);
    expect(fieldEnabled(key, { experimentalActive: false, [key]: true })).toBe(false);
  }
});

it('translates the five labels and their connection or live hints in both languages', () => {
  for (const lang of ['en', 'zh-CN'] as const) {
    for (const [index, key] of choices.entries()) {
      const label = translate(lang, `field.${key}`);
      const hint = translate(lang, `hint.field.${key}`);
      expect(label).not.toBe(`field.${key}`);
      expect(hint).not.toBe(`hint.field.${key}`);
      expect(hint).toContain(index < 3 ? (lang === 'en' ? 'next connection' : '下一次连接') : (lang === 'en' ? 'immediately' : '立即生效'));
      expect(label + hint).not.toMatch(/this phone|this app|这台手机|这个 App|…/);
    }
  }
});
