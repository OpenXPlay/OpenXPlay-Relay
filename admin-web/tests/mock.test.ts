// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-15

import { Readable } from 'node:stream';
import type { IncomingMessage, ServerResponse } from 'node:http';
import type { Connect, ViteDevServer } from 'vite';
import { expect, it, vi } from 'vitest';
import { mockApi } from '../dev-server';

function relay(linux = false, state?: string) {
  let handler: Connect.NextHandleFunction;
  const plugin = mockApi(linux);
  const configure = plugin.configureServer as (server: ViteDevServer) => void;
  configure({ middlewares: { use: (callback: Connect.NextHandleFunction) => { handler = callback; } } } as unknown as ViteDevServer);
  let cookie = '';
  return async (path: string, method = 'GET', body: unknown = {}, guard = true) => {
    const req = Readable.from([JSON.stringify(body)]) as IncomingMessage;
    req.url = `/api/v0${path}`; req.method = method;
    req.headers = { cookie, ...(state ? { referer: `http://localhost/?state=${state}` } : {}), ...(guard ? { 'content-type': 'application/json', 'x-openxplay-admin': '1' } : {}) };
    Object.defineProperty(req, 'socket', { value: { remoteAddress: '127.0.0.1' } });
    const headers: Record<string, string> = {};
    return new Promise<{ status: number; data: any; headers: Record<string, string> }>((resolve, reject) => {
      const res = { statusCode: 200, setHeader: (key: string, value: string) => { headers[key] = value; }, end: (raw: string) => {
        if (headers['Set-Cookie']) cookie = headers['Set-Cookie'].split(';')[0];
        resolve({ status: res.statusCode, data: JSON.parse(raw), headers });
      } };
      handler(req, res as unknown as ServerResponse, error => reject(error ?? new Error('unhandled route')));
    });
  };
}
it('walks password login, change, logout and login with the new password', async () => {
  const call = relay();
  expect((await call('/session')).data).toEqual({ state: 'login', defaultPassword: true });
  expect((await call('/status')).status).toBe(401);
  expect((await call('/login', 'POST', { password: 'openxplay' })).status).toBe(200);
  expect((await call('/session')).data).toEqual({ state: 'ready', defaultPassword: true });
  expect((await call('/password', 'POST', { current: 'wrong', new: 'updated password' })).status).toBe(403);
  expect((await call('/password', 'POST', { current: 'openxplay', new: 'short' })).status).toBe(422);
  expect((await call('/password', 'POST', { current: 'openxplay', new: 'updated password' })).status).toBe(200);
  expect((await call('/session')).data.defaultPassword).toBe(false);
  await call('/logout', 'POST');
  expect((await call('/session')).data).toEqual({ state: 'login', defaultPassword: false });
  expect((await call('/login', 'POST', { password: 'openxplay' })).status).toBe(401);
  expect((await call('/login', 'POST', { password: 'updated password' })).status).toBe(200);
});
it('enforces the mock CSRF guard', async () => {
  expect((await relay()('/login', 'POST', { password: 'openxplay' }, false)).status).toBe(403);
});
it('rate limits after five failed passwords', async () => {
  const call = relay();
  for (let i = 0; i < 5; i++) expect((await call('/login', 'POST', { password: 'wrong' })).status).toBe(401);
  const response = await call('/login', 'POST', { password: 'openxplay' }); expect(response.status).toBe(429); expect(response.headers['Retry-After']).toBeGreaterThan(0);
});
it('applies partial settings atomically and retains validation errors', async () => {
  const call = relay(); await call('/login', 'POST', { password: 'openxplay' });
  const response = await call('/settings', 'PUT', { displayWidth: 1, autoStart: false });
  expect(response.status).toBe(422); expect(response.data.errors.displayWidth).toBe('error.range');
  expect((await call('/settings')).data.values.autoStart).toBe(true);
  expect((await call('/settings', 'PUT', { autoStart: false })).data.applied).toBe('now');
});
it('starts, reports next-start saves, and pauses automatic start on stop', async () => {
  const call = relay(); await call('/login', 'POST', { password: 'openxplay' });
  await call('/actions/start', 'POST'); expect((await call('/status')).data.running).toBe(true);
  expect((await call('/settings', 'PUT', { displayWidth: 1920 })).data.applied).toBe('next-start');
  await call('/actions/stop', 'POST'); expect((await call('/status')).data.autoStartPaused).toBe(true);
});
it('returns phone kinds and selection', async () => {
  const call = relay(); await call('/login', 'POST', { password: 'openxplay' }); await call('/settings', 'PUT', { iphone: 'iphone-riko' });
  const phones = (await call('/phones')).data; expect(phones.map((phone: { kind: string }) => phone.kind)).toEqual(['phone', 'car', 'other']); expect(phones[0].selected).toBe(true);
});

it('provides Linux Wi-Fi status, scans, join and forget', async () => {
  const call = relay(true); await call('/login', 'POST', { password: 'openxplay' });
  const status = (await call('/status')).data;
  expect(status.platform).toBe('linux'); expect(status.wifiProvisioning).toBe(true); expect(status.wifiClient.setupHotspot).toBe(true);
  const settings = (await call('/settings')).data;
  expect(settings.schema.some((section: { id: string }) => section.id === 'wifi')).toBe(true);
  const options = settings.schema.find((section: { id: string }) => section.id === 'network').fields[0].options;
  expect(options.map((option: { value: string }) => option.value)).toEqual(['shared-wifi', 'device-hotspot']);
  expect((await call('/wifi/networks')).data.length).toBe(4);
  expect((await call('/wifi', 'PUT', { ssid: 'Home Wi-Fi', password: 'openxplay' })).status).toBe(200);
  expect((await call('/status')).data.wifiClient).toMatchObject({ ssid: 'Home Wi-Fi', setupHotspot: false });
  await call('/actions/forget-wifi', 'POST');
  expect((await call('/status')).data.wifiClient.setupHotspot).toBe(true);
  expect((await call('/wifi/networks')).data.every((network: { known: boolean }) => !network.known)).toBe(true);
});
it.each([['Home Wi-Fi', 'auth'], ['Join failure: not found', 'notFound'], ['Join failure: timeout', 'timeout']])('retains join failure for %s on the next visit', async (ssid, reason) => {
  const call = relay(true); await call('/login', 'POST', { password: 'openxplay' });
  await call('/wifi', 'PUT', { ssid, password: 'wrong' });
  await call('/logout', 'POST'); await call('/login', 'POST', { password: 'openxplay' });
  const status = (await call('/status')).data;
  expect(status.notices).toEqual([`wifi.joinFailed.${reason}`]); expect(status.wifiClient.setupHotspot).toBe(true);
});
it('joins open networks without a password and clears a previous failure', async () => {
  const call = relay(true); await call('/login', 'POST', { password: 'openxplay' });
  await call('/wifi', 'PUT', { ssid: 'Home Wi-Fi', password: 'wrong' });
  await call('/wifi', 'PUT', { ssid: 'Cafe (open)', password: '' });
  expect((await call('/status')).data).toMatchObject({ notices: [], wifiClient: { ssid: 'Cafe (open)', setupHotspot: false } });
});
it('does not expose Wi-Fi provisioning on the Android mock', async () => {
  const call = relay(); await call('/login', 'POST', { password: 'openxplay' });
  expect((await call('/wifi/networks')).status).toBe(404); expect((await call('/status')).data.wifiProvisioning).toBeUndefined();
});

it('Android lists redact passwords, keep omitted passwords and delete omitted entries', async () => {
  const call = relay(); await call('/login', 'POST', { password: 'openxplay' });
  let values = (await call('/settings')).data.values;
  expect(values.sharedWifiNetworks).toEqual([{ ssid: 'Home Wi-Fi', hasPassword: true, current: false }, { ssid: 'Car Wi-Fi', hasPassword: false, current: false }]);
  expect(values).not.toHaveProperty('wifiPassword');
  expect((await call('/settings', 'PUT', { sharedWifiNetworks: [{ ssid: 'Home Wi-Fi' }, { ssid: 'Other', password: 'new-password' }] })).status).toBe(200);
  values = (await call('/settings')).data.values;
  expect(values.sharedWifiNetworks).toEqual([{ ssid: 'Home Wi-Fi', hasPassword: true, current: false }, { ssid: 'Other', hasPassword: true, current: false }]);
  expect(JSON.stringify(values)).not.toContain('new-password');
  expect((await call('/settings', 'PUT', { sharedWifiNetworks: [{ ssid: 'Home Wi-Fi', password: '' }] })).status).toBe(200);
  expect((await call('/settings')).data.values.sharedWifiNetworks).toEqual([{ ssid: 'Home Wi-Fi', hasPassword: false, current: false }]);
  expect((await call('/settings', 'PUT', { sharedWifiNetworks: [] })).status).toBe(200);
  expect((await call('/settings')).data.values.sharedWifiNetworks).toEqual([]);
});
it('Linux exposes the same saved-network contract', async () => {
  const call = relay(true); await call('/login', 'POST', { password: 'openxplay' });
  expect((await call('/settings')).data.values.sharedWifiNetworks).toHaveLength(2);
  expect((await call('/settings', 'PUT', { sharedWifiNetworks: [] })).status).toBe(200);
});
it('rejects invalid network replacements atomically', async () => {
  const call = relay(); await call('/login', 'POST', { password: 'openxplay' });
  for (const list of [[{ ssid: 'Home', password: 'short' }], [{ ssid: 'Home' }, { ssid: 'Home' }], [{ ssid: 'Home', password: null }]]) {
    expect((await call('/settings', 'PUT', { sharedWifiNetworks: list, autoStart: false })).status).toBe(422);
    expect((await call('/settings')).data.values.autoStart).toBe(true);
  }
});

it('uses the Linux settings schema for Linux preview states', async () => {
  vi.stubEnv('DIRELAY_PREVIEW_STATES', '1');
  try {
    const call = relay(true);
    const settings = (await call('/settings')).data;
    const keys = settings.schema.flatMap((section: { fields: { key: string }[] }) => section.fields.map(field => field.key));
    expect(keys).toContain('wifiInterface');
    expect(keys).not.toContain('autoStart');
    expect(keys).not.toContain('bootStart');
    expect(settings.values).not.toHaveProperty('autoStart');
    expect(settings.values).not.toHaveProperty('bootStart');
    expect(settings.schema.filter((section: { id: string }) => section.id === 'wifi')).toHaveLength(1);
    expect((await call('/status')).data.platform).toBe('linux');
  } finally { vi.unstubAllEnvs(); }
});

it.each([true, false])('applies a network switch now with a two-second outage (Linux %s)', async linux => {
  const call = relay(linux); await call('/login', 'POST', { password: 'openxplay' });
  const now = Date.now(); const time = vi.spyOn(Date, 'now').mockReturnValue(now);
  try {
    const target = linux ? 'device-hotspot' : 'phone-hotspot';
    expect((await call('/settings', 'PUT', { networkMode: target })).data.applied).toBe('now');
    expect((await call('/status')).status).toBe(503);
    time.mockReturnValue(now + 2001);
    expect((await call('/status')).data).toMatchObject({ wifiControl: linux, network: { mode: target, wifi: null }, hotspot: { ssid: 'OpenXPlay', password: 'openxplay123', active: true, ip: '10.42.0.1' } });
    expect((await call('/settings')).data.values.sharedWifiNetworks.every((entry: { current: boolean }) => !entry.current)).toBe(true);
    expect((await call('/settings', 'PUT', { networkMode: 'shared-wifi' })).data.applied).toBe('now');
    time.mockReturnValue(now + 4002);
    expect((await call('/status')).data.network).toEqual({ mode: 'shared-wifi', wifi: 'Home Wi-Fi' });
    await call('/settings', 'PUT', { sharedWifiNetworks: [] });
    expect((await call('/settings', 'PUT', { networkMode: 'shared-wifi' })).data.errors).toEqual({ networkMode: 'error.noSavedWifi' });
  } finally { time.mockRestore(); }
});
it('connects using an omitted saved password, keeps additions disconnected, and redacts credentials', async () => {
  const call = relay(true); await call('/login', 'POST', { password: 'openxplay' });
  expect((await call('/wifi', 'PUT', { ssid: 'Home Wi-Fi' })).status).toBe(200);
  expect((await call('/status')).data.network.wifi).toBe('Home Wi-Fi');
  await call('/settings', 'PUT', { sharedWifiNetworks: [{ ssid: 'Home Wi-Fi' }, { ssid: 'Hidden', password: 'secret123' }] });
  expect((await call('/status')).data.network.wifi).toBe('Home Wi-Fi');
  const entries = (await call('/settings')).data.values.sharedWifiNetworks;
  expect(entries).toEqual([{ ssid: 'Home Wi-Fi', hasPassword: true, current: true }, { ssid: 'Hidden', hasPassword: true, current: false }]);
  await call('/settings', 'PUT', { sharedWifiNetworks: [{ ssid: 'Home Wi-Fi', password: 'changed123' }] });
  expect((await call('/status')).data.wifiClient.setupHotspot).toBe(true);
});
it.each(['working', 'hotspot'])('keeps the %s preview settings aligned with status', async state => {
  vi.stubEnv('DIRELAY_PREVIEW_STATES', '1');
  try {
    const call = relay(true, state); const settings = (await call('/settings')).data;
    expect(settings.values.sharedWifiNetworks).toHaveLength(2);
    expect(settings.values.sharedWifiNetworks.filter((entry: { current: boolean }) => entry.current)).toHaveLength(state === 'working' ? 1 : 0);
    expect((await call('/status')).data).toMatchObject(state === 'working' ? { network: { mode: 'shared-wifi', wifi: 'Car Wi-Fi' } } : { platform: 'linux', wifiControl: true, network: { mode: 'device-hotspot', wifi: null }, hotspot: { ip: '10.42.0.1', active: true } });
  } finally { vi.unstubAllEnvs(); }
});

it('validates Linux hotspot edits behind auth and reports the saved credentials', async () => {
  const call = relay(true);
  const credentials = { ssid: 'Relay-car', password: 'newpass123' };
  expect((await call('/hotspot', 'PUT', credentials)).status).toBe(401);
  await call('/login', 'POST', { password: 'openxplay' });
  expect((await call('/hotspot', 'PUT', credentials, false)).status).toBe(403);
  expect((await call('/hotspot', 'PUT', { ...credentials, ssid: '中'.repeat(11) })).data.error).toBe('error.hotspotSsid');
  expect((await call('/hotspot', 'PUT', { ...credentials, password: 'short' })).data.error).toBe('error.hotspotPassword');
  for (const band of ['6', 5, null]) expect((await call('/hotspot', 'PUT', { ...credentials, band })).status).toBe(422);
  expect((await call('/hotspot', 'PUT', { ...credentials, band: '2.4' })).status).toBe(200);
  const status = (await call('/status')).data;
  expect(status.hotspot).toMatchObject({ ...credentials, hasPassword: true });
  expect(status.wireless).toMatchObject({ preset: { id: 'phicomm-n1', source: 'detected' }, hotspot: { configuredBand: '2.4', band: '2.4', channel: 6 } });
  expect((await call('/settings')).data.values.hotspotBand).toBe('2.4');
});
it('applies Linux output edits immediately while running', async () => {
  const call = relay(true);
  await call('/login', 'POST', { password: 'openxplay' });
  await call('/actions/start', 'POST');
  expect((await call('/settings', 'PUT', { output: 'carlink' })).data.applied).toBe('now');
  expect((await call('/status')).data.output).toBe('carlink');
});
it('opens a ten-minute pairing window that only Linux can close early', async () => {
  const linux = relay(true); await linux('/login', 'POST', { password: 'openxplay' });
  const before = (await linux('/status')).data;
  expect(before).toMatchObject({ pairingAvailable: true, pairingCancelable: true, pairingDeadline: null, pairingRemaining: null });
  expect(before.pairedDevices[0]).toEqual({ name: "Riko's iPhone", connected: true, iphone: true });
  expect((await linux('/actions/pair-iphone', 'POST')).status).toBe(200);
  const open = (await linux('/status')).data;
  expect(open.pairingRemaining).toBeGreaterThan(595); expect(open.pairingRemaining).toBeLessThanOrEqual(600);
  expect((await linux('/actions/cancel-pairing', 'POST')).status).toBe(200);
  expect((await linux('/status')).data.pairingRemaining).toBeNull();
  const android = relay(); await android('/login', 'POST', { password: 'openxplay' });
  expect((await android('/status')).data.pairingCancelable).toBe(false);
  expect((await android('/actions/cancel-pairing', 'POST')).status).toBe(404);
});
it('returns receiver diagnostics only when the developer section asks', async () => {
  const call = relay(true); await call('/login', 'POST', { password: 'openxplay' });
  await call('/actions/start', 'POST');
  expect((await call('/status')).data.receivers).toBeUndefined();
  const status = (await call('/status?diagnostics=1')).data;
  expect(status.receivers).toHaveLength(1);
  expect(status.receivers[0].stats.length).toBeLessThanOrEqual(30);
  expect(status.video).toMatchObject({ codec: 'H265', reason: 'all-main-screen-outputs-hardware-hevc', width: 1920, height: 720 });
});

it('applies Linux plugin switches immediately and validates installed IDs atomically', async () => {
  const call = relay(true); await call('/login', 'POST', { password: 'openxplay' });
  const initial = (await call('/settings')).data;
  expect(initial.values.plugins).toBe(false); expect(initial.values.pluginsEnabled).toEqual([]);
  expect(initial.installedPlugins[1].nonCommercialAssets).toBe(true);
  await call('/actions/start', 'POST');
  expect((await call('/settings', 'PUT', { plugins: true, pluginsEnabled: ['org.openxplay.pet', 'org.openxplay.clock'] })).data.applied).toBe('now');
  for (const ids of [['org.missing'], ['org.openxplay.pet', 'org.openxplay.pet'], 'org.openxplay.pet']) {
    expect((await call('/settings', 'PUT', { plugins: false, pluginsEnabled: ids })).status).toBe(422);
    expect((await call('/settings')).data.values.plugins).toBe(true);
  }
  expect((await call('/settings')).data.values.pluginsEnabled).toEqual(['org.openxplay.pet', 'org.openxplay.clock']);
  expect((await call('/settings', 'PUT', { plugins: false })).data.applied).toBe('now');
});
