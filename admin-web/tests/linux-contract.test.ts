// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-27

import { expect, it } from 'vitest';
import { readFileSync } from 'node:fs';
import { linuxSettings, linuxStatusFixtures, mockSettings } from '../dev-server';
import { optionLabel } from '../src/schema';
import { fieldRoute, menuSummary, pageAvailable } from '../src/settings-tree';
import { presentPath } from '../src/path/present-path';
import { presentStage } from '../src/path/present-stage';
import { translate } from '../src/i18n';
import type { Settings } from '../src/schema';

const carlink = JSON.parse(readFileSync(new URL('./fixtures/linux/settings-carlink.json', import.meta.url), 'utf8')) as Settings;
it.each(['en', 'zh-CN'] as const)('resolves every production Linux field, section, option and hint (%s)', language => {
  const t = (key: string) => translate(language, key);
  for (const settings of [linuxSettings, carlink]) for (const section of settings.schema) {
    expect(t(section.titleKey), section.titleKey).not.toBe(section.titleKey);
    for (const field of section.fields) {
      expect(fieldRoute(field.key), field.key).not.toBe('settings/developer/other');
      for (const key of [field.labelKey, field.hintKey].filter(Boolean) as string[]) expect(t(key), key).not.toBe(key);
      for (const option of field.options ?? []) {
        expect(optionLabel(option, language, t)).not.toBe('');
        if (option.labels) expect(Object.keys(option.labels).sort()).toEqual(['en', 'zh-CN']);
        else expect(t(option.labelKey), option.labelKey).not.toBe(option.labelKey);
        if (option.hintKey) expect(t(option.hintKey), option.hintKey).not.toBe(option.hintKey);
      }
    }
  }
});
it.each(Object.entries(linuxStatusFixtures))('accepts the real Linux %s response in both presenters', (_state, status) => {
  expect(status.adminFeatures).toEqual(['logs', 'config-files', 'update-check', 'identity']);
  expect(status.pathInput?.platform).toBe('linux');
  const path = presentPath(status.pathInput!);
  expect(presentStage(path)).toBeDefined();
  for (const language of ['en', 'zh-CN'] as const) {
    const t = (key: string) => translate(language, key);
    for (const section of path.sections) for (const step of section.steps) {
      for (const key of [step.title, step.explanation, step.actionLabel].filter(Boolean) as string[]) expect(t(key), key).not.toBe(key);
    }
  }
});
it('matches Android menu summaries and hides unavailable or empty pages', () => {
  const status = linuxStatusFixtures.ready, t = (key: string) => translate('en', key);
  expect(menuSummary(linuxSettings, status, 'settings/car', 'en', t)).toBe('');
  expect(menuSummary(carlink, status, 'settings/car', 'en', t)).toBe('XPeng · G6 · 2023');
  expect(menuSummary(linuxSettings, status, 'settings/picture', 'en', t)).toBe('Automatic · 200 mm · Automatic · Auto · 60 fps');
  const android = { ...status, platform: 'android' as const, adminFeatures: undefined };
  for (const route of ['settings/logs', 'settings/config-files', 'settings/experimental']) expect(pageAvailable(mockSettings(), android, route)).toBe(false);
  expect(pageAvailable(linuxSettings, status, 'settings/developer/video')).toBe(true);
  expect(pageAvailable({ ...linuxSettings, schema: [] }, status, 'settings/developer/video')).toBe(false);
});

it('retains the backend arrival, armed and frozen failure contracts for the startup UI', () => {
  const arrival = linuxStatusFixtures.arrival.pathInput!;
  expect(arrival.startup?.arrival).toEqual({ pending: true, deadlineMillis: arrival.nowMillis! + 5000, remainingMillis: 5000 });
  expect(linuxStatusFixtures.armed.pathInput!.iphone!.armed).toBe(true);
  const failure = linuxStatusFixtures['iphone-failure'].pathInput!.iphone!.failure!;
  expect(failure.text).toBe('rfcomm timeout');
  expect(failure.trail.some(line => line.startsWith('+2.000s '))).toBe(true);
  expect(linuxSettings.values.bootStart).toBe(true);
});

it('presents an unreachable iPhone as waiting in English and Simplified Chinese', () => {
  const input = linuxStatusFixtures['iphone-waiting'].pathInput!;
  expect(input.iphone!.step).toBe('WAITING_IPHONE');
  expect(input.iphone!.failure).toBeNull();
  const path = presentPath(input), stage = presentStage(path);
  expect(path.running).toBe(true);
  expect(path.tone).toBe('PROGRESS');
  expect(stage.failure).toBeNull();
  expect(stage.headline).toEqual({ res: 'state.waiting', args: [] });
  expect(translate('en', stage.headline.res!)).toBe('Waiting for iPhone');
  expect(translate('zh-CN', stage.headline.res!)).toBe('等待 iPhone');
});
