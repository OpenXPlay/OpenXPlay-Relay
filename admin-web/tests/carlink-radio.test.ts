// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-31

import { it, expect } from 'vitest';
import { carlinkRadioMissing, outputOptionDisabled, carlinkRadioBlocker, browserOutput } from '../src/carlink-radio';
import type { Status } from '../src/api';
import { translate } from '../src/i18n';

it('gates Linux wizard and Settings options, preserves Android choices, and recovers on hotplug', () => {
  const status = { platform: 'linux', output: 'both', network: { plan: { carlink: { radioAvailable: false } } } } as Status;
  for (const language of ['en', 'zh-CN'] as const) {
    const t = (key: string) => translate(language, key);
    expect(carlinkRadioBlocker(status, t)).toContain('data-browser-output');
    expect(carlinkRadioBlocker(status, t)).toContain(t('carlink.radioMissing'));
  }
  expect(browserOutput).toEqual({ output: 'web' });
  expect(outputOptionDisabled(status, 'web')).toBe(false);
  for (const value of ['carlink', 'both']) expect(outputOptionDisabled(status, value)).toBe(true);
  status.network.plan!.carlink.radioAvailable = true;
  expect(carlinkRadioMissing(status)).toBe(false);
  expect(carlinkRadioBlocker(status, (key: string) => translate('en', key))).toBe('');
  status.network.plan!.carlink.radioAvailable = false;
  status.platform = 'android';
  expect(outputOptionDisabled(status, 'both')).toBe(false);
  expect(carlinkRadioBlocker(status, (key: string) => translate('en', key))).toBe('');
});
