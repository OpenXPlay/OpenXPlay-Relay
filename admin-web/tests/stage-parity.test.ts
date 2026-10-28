// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-27

import { expect, it, vi } from 'vitest';
import { api } from '../src/api';
import { arrivalCountdown, arrivalRemaining, restartPending } from '../src/startup';
import { setupSteps, setupBlocked, preparationView } from '../src/setup-guide';
import { presentPath } from '../src/path/present-path';
import { normalizeInput } from '../src/path/model';
import { stepCommand } from '../src/path/actions';
import { stepDetailsView, stepTrail } from '../src/step-details';
import { relayTranslate } from '../src/i18n';
import { addressesView } from '../src/views';
import { fieldRoute } from '../src/settings-tree';
import { linuxSettings, linuxStatusFixtures } from '../dev-server';

it('offers only the runtime arrival and honors paused, automatic, readiness, running and busy gates', () => {
  const input = normalizeInput(linuxStatusFixtures.arrival.pathInput);
  const path = presentPath(input);
  expect(arrivalCountdown(input.startup, path)).toBe(true);
  expect(arrivalCountdown(input.startup, path, true)).toBe(false);
  for (const startup of [{ ...input.startup!, paused: true }, { ...input.startup!, automatic: false }, { ...input.startup!, arrival: { ...input.startup!.arrival, pending: false } }]) expect(arrivalCountdown(startup, path)).toBe(false);
  expect(arrivalCountdown(input.startup, { ...path, running: true })).toBe(false);
  expect(arrivalCountdown(input.startup, { ...path, canStart: false })).toBe(false);
  expect(arrivalCountdown(undefined, path)).toBe(false);
  expect(input.startup?.arrival.deadlineMillis! - input.nowMillis).toBe(5000);
  input.startup!.arrival.deadlineMillis = 9000000000000;
  expect(arrivalRemaining(input.startup, input.nowMillis, 2100)).toBe(2900);
  expect(arrivalRemaining(input.startup, input.nowMillis, 6000)).toBe(0);
});
it('reconciles the Linux restart latch with runtime notices and retains Android local feedback', () => {
  const s = structuredClone(linuxStatusFixtures.ready);
  expect(restartPending(s, true)).toBe(false);
  s.notices = ['notice.nextStart']; expect(restartPending(s, false)).toBe(true);
  s.notices = []; delete s.pathInput; expect(restartPending({ ...s, platform: 'android' }, true)).toBe(true);
});
it('keeps setup sequential, validates the car and network, and uses the same readiness as Connect', () => {
  expect(setupSteps).toEqual(['car', 'output', 'connection', 'network', 'prepare', 'start']);
  expect(setupBlocked('car', linuxSettings, linuxStatusFixtures.ready)).toBe(true);
  expect(setupBlocked('prepare', linuxSettings, linuxStatusFixtures['identity-missing'])).toBe(true);
  expect(setupBlocked('prepare', linuxSettings, linuxStatusFixtures.ready)).toBe(false);
  expect(setupBlocked('start', linuxSettings, linuxStatusFixtures['live-browser'])).toBe(false);
  expect(setupBlocked('network', { ...linuxSettings, values: { ...linuxSettings.values, networkMode: 'shared-wifi', sharedWifiNetworks: [] } }, linuxStatusFixtures.ready)).toBe(true);
});
it.each(['en', 'zh-CN'] as const)('shows action, recheck, code, timestamp and complete copyable evidence/trail (%s)', language => {
  const t = (key: string, args?: Record<string, string | number>) => relayTranslate(language, key, args);
  const step = presentPath(linuxStatusFixtures['iphone-failure'].pathInput!).sections.flatMap(s => s.steps).find(s => s.code === 'rfcomm_timeout')!;
  const markup = stepDetailsView(step, [], 'linux', t);
  for (const part of ['rfcomm_timeout', '<time', 'data-action', 'data-recheck', 'data-evidence', '+2.000s']) expect(markup).toContain(part);
  expect(markup).not.toContain('this phone'); expect(markup).not.toContain('这台手机');
  const page = presentPath(linuxStatusFixtures.ready.pathInput!).sections.flatMap(s => s.steps).find(s => s.id === 'CAR_PAGE')!;
  expect(stepTrail(page, ['https://a/', 'https://b/', 'https://a/'])).toEqual(expect.arrayContaining(['https://a/', 'https://b/']));
  const guide = preparationView(linuxStatusFixtures['bluetooth-off'], t);
  expect(guide).toContain('data-prepare-action="BLUETOOTH"');
  for (const key of ['hint.resolution', 'hint.frameRate', 'hint.videoCodec', 'hint.field.bootStart', 'admin.recovery', 'setup2_prepare_subtitle', 'iphone_pair_hint']) expect(t(key), key).not.toBe(key);
});
it('keeps every advertised page address, including names with no matching interface', () => {
  const s = structuredClone(linuxStatusFixtures.ready); s.receiverUrls.push('https://named.example:8443/?code=1234');
  s.hostAddresses!.push({ interface: 'eth0', ipv4: '192.168.1.2', developerOnly: false });
  const html = addressesView(s, true, key => key);
  for (const value of [...s.receiverUrls, 'address-network', 'data-address="1"']) expect(html).toContain(value);
});
it('places the Carlink identity and combined output controls in Developer Connection', () => {
  expect(fieldRoute('carlinkModel')).toBe('settings/developer/connection');
  expect(fieldRoute('carlinkPictureSizes')).toBe('settings/picture');
  expect(stepCommand('TURN_ON_BLUETOOTH', 'linux')).toEqual({ kind: 'api', endpoint: 'actions/enable-bluetooth' });
  expect(stepCommand('OPEN_WIFI', 'linux')).toEqual({ kind: 'api', endpoint: 'actions/enable-wifi' });
});
it('uses confirmed authenticated access/reset routes and never retries a lost arrival cancellation', async () => {
  const fetch = vi.fn().mockResolvedValue(new Response('{}')); vi.stubGlobal('fetch', fetch);
  try {
    await api.access(false); await api.resetPassword(); await api.completeSetup();
    expect(fetch.mock.calls.map(([url, init]) => [url, JSON.parse(init.body)])).toEqual([
      ['/api/v0/access', { enabled: false, confirmed: true }], ['/api/v0/password/reset', { confirmed: true }], ['/api/v0/setup/complete', {}],
    ]);
    fetch.mockClear().mockRejectedValue(new TypeError('lost response'));
    await expect(api.action('cancel-arrival')).rejects.toThrow(); expect(fetch).toHaveBeenCalledOnce();
  } finally { vi.unstubAllGlobals(); }
});
