// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-27

import { expect, it } from 'vitest';
import { linuxSettings, linuxStatusFixtures } from '../dev-server';
import { translate } from '../src/i18n';
import { teslaAddressKeys, teslaHint, teslaNotes } from '../src/tesla';
import { addressViews } from '../src/logic';
import { fieldRoute, normalizeRoute, pageAvailable, pageFields, menuSummary } from '../src/settings-tree';
import type { Status } from '../src/api';
import { presentPath } from '../src/path/present-path';

it.each(['en', 'zh-CN'] as const)('offers the five modes in a top-level Tesla page with formatted hints (%s)', language => {
  const t = (key: string, params?: Record<string, string | number>) => translate(language, key, params);
  expect(normalizeRoute('#settings/tesla')).toBe('settings/tesla');
  expect(pageAvailable({ ...linuxSettings, values: { ...linuxSettings.values, carBrand: 'tesla' } }, linuxStatusFixtures.ready, 'settings/tesla')).toBe(true);
  const fields = pageFields(linuxSettings, 'settings/tesla');
  const mode = fields.find(f => f.key === 'teslaBrowser')!;
  expect(mode.options?.map(o => o.value)).toEqual(['teslamirror', 'virtual-name', 'public-address', 'tesla-android', 'off']);
  for (const [mode, key] of Object.entries(teslaAddressKeys)) {
    expect(fieldRoute(key)).toBe('settings/tesla');
    expect(teslaHint(linuxSettings.values, mode, t)).toContain(linuxSettings.values[key]);
    const settings = { ...linuxSettings, values: { ...linuxSettings.values, teslaBrowser: mode } };
    expect(menuSummary(settings, linuxStatusFixtures.ready, 'settings/tesla', language, t)).toBe(t(`tesla.mode.${mode}`));
  }
  expect(teslaHint(linuxSettings.values, 'off', t)).toBe('');
  expect(t('tesla.intro')).not.toMatch(/VPN|phone|手机/);
  expect(t('tesla.requirements')).not.toMatch(/Wi-Fi Direct|Wi-Fi 直连/);
});
it.each(['en', 'zh-CN'] as const)('links the unmet requirements to their settings and escapes failures (%s)', language => {
  const t = (key: string, params?: Record<string, string | number>) => translate(language, key, params);
  const status = structuredClone(linuxStatusFixtures.ready);
  const values = { ...linuxSettings.values, teslaBrowser: 'teslamirror', output: 'carlink', networkMode: 'shared-wifi' };
  const notes = teslaNotes(values, status, t);
  for (const route of ['output', 'connection/network', 'access/public-name']) expect(notes).toContain(`#settings/${route}`);
  expect(teslaNotes({ ...values, teslaBrowser: 'off' }, status, t)).toBe('');
  expect(teslaNotes({ ...values, teslaBrowser: 'tesla-android', output: 'web', networkMode: 'device-hotspot' }, status, t)).not.toContain('#settings/');
  status.pathInput!.car!.tesla = { phase: 'STARTING', name: null, code: null, sinceMillis: 10, activation: 'NEEDS_RELAY_HOTSPOT' };
  expect(teslaNotes({ ...values, output: 'web', networkMode: 'device-hotspot' }, status, t)).toContain('#settings/connection/network');
  status.pathInput!.car!.tesla = { phase: 'FAILED', name: null, code: '<bad>', sinceMillis: 10 };
  expect(teslaNotes(values, status, t)).toContain('&lt;bad&gt;');
});
it('puts the ready Tesla page first for named and direct modes without losing the other addresses', () => {
  for (const url of ['https://169.254.88.1:8443/', 'https://tesla.example.org:8443/']) {
    const status: Status = structuredClone(linuxStatusFixtures.ready);
    const original = addressViews(status, false);
    status.pathInput!.car!.tesla = { phase: 'READY', name: url, code: null, sinceMillis: 10 };
    status.receiverUrls.unshift(url);
    if (url.includes('example')) status.publicName = { state: 'ready', trustedName: 'tesla.example.org', address: '100.88.0.1' };
    const addresses = addressViews(status, false);
    expect(addresses[0].receiver).toBe(url);
    expect(addresses[0].label).toBe('tesla.title');
    expect(addresses.slice(1)).toEqual(original);
  }
});
it('shows an address failure on the Tesla path step while the iPhone keeps playing', () => {
  const input = structuredClone(linuxStatusFixtures['live-browser'].pathInput!);
  input.setup!.outputs.push('TESLA');
  input.car!.tesla = { phase: 'FAILED', name: null, code: 'NM address timeout', sinceMillis: 10 };
  const path = presentPath(input);
  const steps = path.sections.flatMap(s => s.steps);
  expect(steps.find(s => s.id === 'TESLA')?.state).toBe('FAILED');
  expect(input.live?.pictureOnCar).toBe(true);
});
