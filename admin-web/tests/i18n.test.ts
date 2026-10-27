// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-15

import { expect, it } from 'vitest';
import { defaultLanguage, translate } from '../src/i18n';
import { mockSettings } from '../dev-server';
it('uses the key itself for unknown messages in both languages', () => { expect(translate('en', 'future.key')).toBe('future.key'); expect(translate('zh-CN', 'future.key')).toBe('future.key'); });
it('selects Chinese for Chinese navigator locales', () => { expect(defaultLanguage('zh-TW')).toBe('zh-CN'); expect(defaultLanguage('zh-CN')).toBe('zh-CN'); expect(defaultLanguage('fr-FR')).toBe('en'); });
it('translates labels in both languages', () => { expect(translate('en', 'field.networkMode')).toBe('Network mode'); expect(translate('zh-CN', 'field.networkMode')).toBe('网络模式'); });
it('substitutes rate-limit seconds', () => expect(translate('en', 'error.rateLimit', { seconds: 42 })).toContain('42 seconds'));
it('covers every mock schema key and all Linux field labels', () => {
  const keys = [mockSettings(), mockSettings(true)].flatMap(settings => settings.schema).flatMap(section => [section.titleKey, ...section.fields.flatMap(field => [field.labelKey, field.hintKey, ...(field.options?.flatMap(option => [option.labelKey, option.hintKey]) ?? [])])]).filter((key): key is string => !!key);
  keys.push('wifi.joinFailed.auth', 'wifi.joinFailed.notFound', 'wifi.joinFailed.timeout', 'field.wifiInterface', 'field.wifiChannel', 'field.rfcommChannel', 'hint.rfcommChannel');
  for (const key of keys) for (const lang of ['en', 'zh-CN'] as const) expect(translate(lang, key), `${lang}: ${key}`).not.toBe(key);
});


import { readFileSync } from 'node:fs';
it('keeps English and Chinese keys and substitution parameters in parity', () => {
  const source = readFileSync(new URL('../src/i18n.ts', import.meta.url), 'utf8');
  const english = source.split('"en": {')[1].split('"zh-CN": {')[0];
  const chinese = source.split('"zh-CN": {')[1].split('export function')[0];
  const entries = (text: string) => new Map([...text.matchAll(/"([\w./-]+)": "(.*)"/g)].map(([, key, value]) => [key, [...value.matchAll(/\{(\w+)\}/g)].map(m => m[1]).sort()]));
  const en = entries(english); const zh = entries(chinese);
  expect([...en.keys()].sort()).toEqual([...zh.keys()].sort());
  for (const [key, parameters] of en) expect(zh.get(key), key).toEqual(parameters);
});
it('translates every pairing, video, Bluetooth guard and diagnostics key in English and Chinese', async () => {
  const { VIDEO_REASONS } = await import('../src/diagnostics');
  const sources = ['pairing.ts', 'diagnostics.ts', 'views.ts', 'main.ts'].map(file => readFileSync(new URL(`../src/${file}`, import.meta.url), 'utf8')).join('\n');
  const literal = [...sources.matchAll(/\bt\('((?:pairing|video|diag|bt)\.[\w.-]+)'/g)].map(m => m[1]);
  const mapped = [...sources.matchAll(/'((?:diag|bt)\.[\w.-]+)'/g)].map(m => m[1]);
  const computed = [...VIDEO_REASONS.map(r => `video.reason.${r}`), 'video.mismatch.software', 'video.mismatch.none', 'bt.state.ok', 'bt.state.recovering', 'bt.state.failed', 'home.pairCountdown', 'error.pairing', 'error.pairingPermission', 'error.pairingOpenApp'];
  const keys = [...new Set([...literal, ...mapped, ...computed])];
  expect(keys.length).toBeGreaterThan(60);
  for (const key of keys) for (const lang of ['en', 'zh-CN'] as const) expect(translate(lang, key), `${lang}: ${key}`).not.toBe(key);
  expect(translate('en', 'pairing.when')).toBe('Use this when pairing a new iPhone or after the iPhone forgot this relay.');
  expect(translate('zh-CN', 'home.pairCountdown', { time: '9:41' })).toContain('9:41');
});
