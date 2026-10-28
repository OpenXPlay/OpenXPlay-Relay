// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-15

import { Readable } from 'node:stream';
import type { IncomingMessage, ServerResponse } from 'node:http';
import type { Connect, ViteDevServer } from 'vite';
import { expect, it, vi } from 'vitest';
import { mockApi } from '../dev-server';

function relay(linux = false, state?: string, peer = '127.0.0.1') {
  let handler: Connect.NextHandleFunction;
  const plugin = mockApi(linux);
  const configure = plugin.configureServer as (server: ViteDevServer) => void;
  configure({ middlewares: { use: (callback: Connect.NextHandleFunction) => { handler = callback; } } } as unknown as ViteDevServer);
  let cookie = '';
  return async (path: string, method = 'GET', body: unknown = {}, guard = true) => {
    const req = Readable.from([JSON.stringify(body)]) as IncomingMessage;
    req.url = `/api/v0${path}`; req.method = method;
    req.headers = { cookie, ...(state ? { referer: `http://localhost/?state=${state}` } : {}), ...(guard ? { 'content-type': 'application/json', 'x-openxplay-admin': '1' } : {}) };
    Object.defineProperty(req, 'socket', { value: { remoteAddress: peer } });
    const headers: Record<string, string> = {};
    return new Promise<{ status: number; data: any; headers: Record<string, string> }>((resolve, reject) => {
      const res = { statusCode: 200, setHeader: (key: string, value: string) => { headers[key] = value; }, end: (raw: string | Buffer) => {
        if (headers['Set-Cookie']) cookie = headers['Set-Cookie'].split(';')[0];
        resolve({ status: res.statusCode, data: headers['Content-Type'] === 'application/json' ? JSON.parse(String(raw)) : raw, headers });
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
  expect(settings.schema.some((section: { id: string }) => section.id === 'connection')).toBe(true);
  const options = settings.schema.flatMap((section: { fields: { key: string; options: unknown }[] }) => section.fields).find((field: { key: string }) => field.key === 'networkMode').options;
  expect(options.map((option: { value: string }) => option.value)).toEqual(['device-hotspot', 'shared-wifi']);
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
    expect(keys).toContain('autoStart');
    expect(keys).toContain('bootStart');
    expect(settings.values.autoStart).toBe(true);
    expect(settings.values.bootStart).toBe(true);
    expect(settings.schema.filter((section: { id: string }) => section.id === 'connection')).toHaveLength(1);
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
    expect((await call('/status')).data.network).toMatchObject({ mode: 'shared-wifi', wifi: 'Home Wi-Fi' });
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

it('serves all Linux trace types, downloads and mutable capture/clear preview state', async () => {
  vi.stubEnv('DIRELAY_PREVIEW_STATES', '1');
  try {
    const call = relay(true, 'working');
    expect((await call('/settings')).data.values.carlinkTrace).toBe('on');
    const snapshot = (await call('/carlink-trace')).data;
    expect(snapshot).toMatchObject({ enabled: true, running: true, status: { dropped: 3, capture: { state: 'idle' } } });
    expect(snapshot.files.map((file: { kind: string }) => file.kind)).toEqual(['trace', 'media']);
    const lines = (await call('/carlink-trace/tail')).data.lines as string[];
    const types = ['Rtsp', 'Frame', 'Socket', 'Negotiation', 'Media', 'Video', 'Input', 'Error', 'Link'];
    expect(new Set(lines.map(line => JSON.parse(line).type))).toEqual(new Set(types));
    for (const type of types) {
      const filtered = (await call(`/carlink-trace/tail?type=${type}&limit=1`)).data.lines;
      expect(filtered).toHaveLength(1); expect(JSON.parse(filtered[0]).type).toBe(type);
    }
    expect((await call('/carlink-trace/tail?limit=0')).data.lines).toEqual([]);
    expect((await call('/carlink-trace/tail?limit=2')).data.lines).toEqual(lines.slice(0, 2));
    for (const query of ['limit=5001', 'type=Errors']) expect((await call('/carlink-trace/tail?' + query)).status).toBe(422);
    const bundle = await call('/carlink-trace/download');
    expect(bundle.headers['Content-Type']).toBe('application/x-ndjson');
    expect(Number(bundle.headers['Content-Length'])).toBe(Buffer.byteLength(bundle.data));
    expect(String(bundle.data).trim().split('\n').map(line => JSON.parse(line))).toHaveLength(lines.length);
    expect((await call(`/carlink-trace/files/${snapshot.files[0].name}`)).data).toEqual(bundle.data);
    const media = await call(`/carlink-trace/files/${snapshot.files[1].name}`);
    expect(media.headers['Content-Type']).toBe('video/mp2t'); expect(media.data[0]).toBe(0x47);
    for (const [action, state] of [['start', 'capturing'], ['stop', 'idle']]) {
      await call('/carlink-trace/capture', 'POST', { action });
      expect((await call('/carlink-trace')).data.status.capture.state).toBe(state);
    }
    await call('/settings', 'PUT', { carlinkTrace: 'off' });
    expect((await call('/carlink-trace/capture', 'POST', { action: 'start' })).status).toBe(409);
    await call('/carlink-trace/clear', 'POST');
    expect((await call('/carlink-trace')).data.files).toEqual([]);
    expect((await call('/carlink-trace/tail')).data.lines).toEqual([]);
    expect((await call(`/carlink-trace/files/${snapshot.files[0].name}`)).status).toBe(404);
  } finally { vi.unstubAllEnvs(); }
});
it('keeps trace routes absent from the Android mock', async () => {
  const call = relay(); await call('/login', 'POST', { password: 'openxplay' });
  for (const path of ['', '/tail', '/download', '/files/trace-00000001-0-00000001.jsonl']) expect((await call('/carlink-trace' + path)).status).toBe(404);
  for (const path of ['/clear', '/capture']) expect((await call('/carlink-trace' + path, 'POST', { action: 'start' })).status).toBe(404);
});
it.each([false, true])('exports valid log ZIPs, retains a receipt and previews both config envelopes (Linux %s)', async linux => {
  const call = relay(linux); await call('/login', 'POST', { password: 'openxplay' });
  expect((await call('/logs/receipt')).status).toBe(404);
  const file = await call('/logs/export', 'POST', { range: '10m' }); expect(file.data).toMatchObject({ bytes: expect.any(Number), sha256: expect.stringMatching(/^[0-9a-f]{64}$/) });
  const zip = await call('/logs/export/' + file.data.id); expect(zip.headers['Content-Type']).toBe('application/zip'); expect(Buffer.from(zip.data).readUInt32LE()).toBe(0x04034b50); expect(Buffer.from(zip.data).toString()).toContain('timeline.txt');
  let receipt = await call('/logs/upload', 'POST', { id: file.data.id });
  if (linux) {
    expect(receipt.data).toMatchObject({ id: expect.any(String), phase: 'Connecting', state: 'running', receipt: null });
    const time = vi.spyOn(Date, 'now').mockReturnValue(Date.now() + 4000);
    try { receipt = { ...receipt, data: (await call('/logs/upload/' + receipt.data.id)).data.receipt }; } finally { time.mockRestore(); }
  }
  expect(receipt.data).toMatchObject({ logId: expect.any(String), expiresMillis: expect.any(Number) }); expect((await call('/logs/receipt')).data).toEqual(receipt.data);
  const png = await call('/logs/qr.png'); expect(Buffer.from(png.data).slice(0, 8)).toEqual(Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]));
  const envelope = (await call('/config/user')).data; expect(envelope.format).toBe('openxplay-user-config');
  envelope.settings.autoStart = false; envelope.settings.futureSetting = true;
  const dry = await call('/config/import?dryRun=1', 'POST', envelope); expect(dry.data.applied).toContain('autoStart'); expect(dry.data.ignored).toContain('futureSetting'); expect((await call('/settings')).data.values.autoStart).toBe(true);
  await call('/config/import', 'POST', envelope); expect((await call('/settings')).data.values.autoStart).toBe(false);
  const privacy = { ...(await call('/config/privacy')).data, providers: { duckdns: { name: '', token: 'private-mock-token' } }, futurePrivate: 'ignored' };
  expect(privacy.format).toBe('openxplay-privacy-config'); expect(privacy).not.toHaveProperty('settings');
  const preview = await call('/config/import?dryRun=1', 'POST', privacy); expect(preview.data).toMatchObject({ applied: ['providers'], ignored: ['futurePrivate'] }); expect((await call('/config/privacy')).data).not.toHaveProperty('providers');
  await call('/config/import', 'POST', privacy); expect((await call('/config/privacy')).data.providers).toEqual(privacy.providers);
  expect((await call('/update?channel=stable')).data).toMatchObject({ current: '0.2.1', latest: '0.2.1', channel: 'stable' }); expect((await call('/update?channel=beta')).data.latest).toContain('beta');
});
it('caps log exports at two files and rejects invalid import envelopes and oversized files', async () => {
  const call = relay(true); await call('/login', 'POST', { password: 'openxplay' });
  const first = (await call('/logs/export', 'POST', { range: '10m' })).data.id;
  await call('/logs/export', 'POST', { range: '30m' }); await call('/logs/export', 'POST', { range: '24h' }); expect((await call('/logs/export/' + first)).status).toBe(404);
  expect((await call('/logs/export', 'POST', { range: 'forever' })).status).toBe(422);
  expect((await call('/config/import', 'POST', { format: 'unknown', version: 1 })).status).toBe(422);
  expect((await call('/config/import', 'POST', { note: 'x'.repeat(65536) })).status).toBe(413);
});
it('validates frame rate, experimental address and picture sizes, and keeps a trial out of saved settings', async () => {
  const call = relay(true); await call('/login', 'POST', { password: 'openxplay' });
  expect((await call('/settings', 'PUT', { frameRate: '60' })).status).toBe(200);
  expect((await call('/settings', 'PUT', { frameRate: '90' })).status).toBe(422);
  for (const address of ['00:00:00:00:00:00', '02:00:00:00:00:00', '01:23:45:67:89:ab', 'not-an-address']) expect((await call('/settings', 'PUT', { experimentalBluetoothAddress: address })).status).toBe(422);
  expect((await call('/settings', 'PUT', { experimentalBluetoothAddress: '00:1A:7D:DA:71:13' })).status).toBe(200);
  expect((await call('/settings', 'PUT', { carlinkPictureSizes: [{ width: 1300, height: 720 }] })).status).toBe(200);
  expect((await call('/settings', 'PUT', { carlinkPictureSizes: [{ width: 1301, height: 720 }] })).status).toBe(422);
  expect((await call('/settings', 'PUT', { carlinkPictureTry: null })).status).toBe(422);
  await call('/settings', 'PUT', { output: 'carlink' });
  const original = (await call('/settings')).data.values.carlinkPictureDefault;
  expect((await call('/settings', 'PUT', { carlinkPictureTry: { width: 1920, height: 720 } })).status).toBe(200);
  const saved = (await call('/settings')).data.values; expect(saved.carlinkPictureTry).toEqual({ width: 1920, height: 720 });
  expect((await call('/config/user')).data.settings).not.toHaveProperty('carlinkPictureTry'); expect(saved.carlinkPictureDefault).toEqual(original);
});
it('expires log exports after one hour', async () => {
  const time = vi.spyOn(Date, 'now').mockReturnValue(1793000000000);
  try {
    const call = relay(true); await call('/login', 'POST', { password: 'openxplay' });
    const file = (await call('/logs/export', 'POST', { range: '10m' })).data;
    time.mockReturnValue(1793003600001); expect((await call('/logs/export/' + file.id)).status).toBe(404);
  } finally { time.mockRestore(); }
});

it('keeps saved hotspot passwords across rename and security changes', async () => {
  const call = relay(true);
  await call('/login', 'POST', { password: 'openxplay' });
  await call('/hotspot', 'PUT', { ssid: 'First', password: 'retained-password', security: 'wpa2' });
  expect((await call('/hotspot', 'PUT', { ssid: 'Renamed', password: '', security: 'open' })).status).toBe(200);
  const open = (await call('/status')).data.hotspot;
  expect(open).toMatchObject({ ssid: 'Renamed', password: 'retained-password', security: 'open', hasPassword: false });
  expect((await call('/hotspot', 'PUT', { ssid: 'Secured', password: '', security: 'wpa3' })).status).toBe(200);
  expect((await call('/status')).data.hotspot).toMatchObject({ ssid: 'Secured', password: 'retained-password', security: 'wpa3', hasPassword: true });
  expect((await call('/hotspot', 'PUT', { ssid: 'Rejected', password: '', security: 'unknown' })).status).toBe(422);
  expect((await call('/status')).data.hotspot.ssid).toBe('Secured');
});

it('keeps arrival deadlines stable through polls, cancellation and subsequent startup phases', async () => {
  vi.stubEnv('DIRELAY_PREVIEW_STATES', '1'); const clock = vi.spyOn(Date, 'now'); let now = 100000; clock.mockImplementation(() => now);
  try {
    const call = relay(true, 'arrival');
    const first = (await call('/status')).data.pathInput;
    now += 2000; const second = (await call('/status')).data.pathInput;
    expect(first.startup.arrival.deadlineMillis).toBe(second.startup.arrival.deadlineMillis);
    expect(second.startup.arrival.remainingMillis).toBe(3000);
    await call('/actions/cancel-arrival', 'POST'); now += 6000;
    const stopped = (await call('/status')).data;
    expect(stopped.running).toBe(false); expect(stopped.pathInput.startup.arrival.pending).toBe(false); expect(stopped.autoStartPaused).toBe(true);
    await call('/actions/start', 'POST');
    expect((await call('/status')).data.pathInput.iphone.step).toBe('BLUETOOTH');
    now += 1100; expect((await call('/status')).data.pathInput.iphone.step).toBe('AUTH');
    now += 1100; expect((await call('/status')).data.pathInput.iphone.step).toBe('JOINING');
    now += 1100; expect((await call('/status')).data.pathInput.live.pictureOnCar).toBe(true);
    await call('/actions/stop', 'POST'); expect((await call('/status')).data.pathInput.iphone.started).toBe(false);
  } finally { clock.mockRestore(); vi.unstubAllEnvs(); }
});
it.each([['bluetooth-off', 'enable-bluetooth', 'bluetoothOn'], ['wifi-off', 'enable-wifi', 'wifiOn']])('repairs the %s preview using live radio facts', async (state, action, fact) => {
  vi.stubEnv('DIRELAY_PREVIEW_STATES', '1');
  try { const call = relay(true, state); expect((await call('/status')).data.pathInput.relay[fact]).toBe(false); await call('/actions/' + action, 'POST'); expect((await call('/status')).data.pathInput.relay[fact]).toBe(true); }
  finally { vi.unstubAllEnvs(); }
});
it('refreshes several phones, selection and a new pairing without replacing the picker session', async () => {
  vi.stubEnv('DIRELAY_PREVIEW_STATES', '1'); const time = vi.spyOn(Date, 'now'); let now = 100000; time.mockImplementation(() => now);
  try {
    const call = relay(true, 'multiple-iphones');
    expect((await call('/status')).data.pathInput.iphone.pick).toBe('MULTIPLE');
    await call('/settings', 'PUT', { iphone: 'iphone-second' });
    expect((await call('/status')).data.pathInput.iphone).toMatchObject({ pick: 'PICKED', name: 'Second iPhone' });
    await call('/actions/pair-iphone', 'POST'); now += 2100;
    expect((await call('/phones')).data.some((phone: { id: string }) => phone.id === 'iphone-new')).toBe(true);
  } finally { time.mockRestore(); vi.unstubAllEnvs(); }
});
it('models replacement approval, incorrect PIN, accepted PIN and real cancellation', async () => {
  vi.stubEnv('DIRELAY_PREVIEW_STATES', '1');
  try {
    const call = relay(true, 'carlink-confirm');
    expect((await call('/status')).data.carlink).toMatchObject({ pinRequest: 18, pinConfirmation: true });
    expect((await call('/carlink/pairing', 'POST', { action: 'pin', request: 17, pin: '1234' })).status).toBe(409);
    await call('/carlink/pairing', 'POST', { action: 'approve', request: 18 });
    expect((await call('/status')).data.carlink).toMatchObject({ pinRequest: 17, pinConfirmation: false });
    await call('/carlink/pairing', 'POST', { action: 'pin', request: 17, pin: '0000' });
    expect((await call('/status')).data.carlink.pinIncorrect).toBe(true);
    await call('/carlink/pairing', 'POST', { action: 'pin', request: 17, pin: '1234' });
    expect((await call('/status')).data.pathInput.car.carlink).toMatchObject({ pinPending: false, trusted: true });
    const cancel = relay(true, 'carlink-pin'); await cancel('/status');
    await cancel('/carlink/pairing', 'POST', { action: 'cancel', request: 17 });
    expect((await cancel('/status')).data.carlink.pinRequested).toBe(false);
    expect((await cancel('/status')).data.pathInput.car.carlink.phase).toBe('ENDED');
  } finally { vi.unstubAllEnvs(); }
});
it('keeps output and network facts aligned after preview edits and clears the restart latch', async () => {
  vi.stubEnv('DIRELAY_PREVIEW_STATES', '1');
  try {
    const call = relay(true, 'live'); await call('/status');
    await call('/settings', 'PUT', { output: 'carlink' });
    let s = (await call('/status')).data; expect(s.pathInput.setup.outputs).toEqual(['CARLINK']); expect(s.pathInput.car.web.phase).toBe('OFF');
    await call('/settings', 'PUT', { frameRate: '30' }); expect((await call('/status')).data.notices).toContain('notice.nextStart');
    await call('/actions/restart', 'POST'); expect((await call('/status')).data.notices).not.toContain('notice.nextStart');
    expect((await call('/settings', 'PUT', { autoStart: false, bootStart: true })).data.applied).toBe('now');
    s = (await call('/status')).data; expect(s.pathInput.startup).toMatchObject({ automatic: false, bootStart: true });
  } finally { vi.unstubAllEnvs(); }
});
it('models confirmed admin reset and disabled head-unit access', async () => {
  const call = relay(true, undefined, '10.42.0.2'); await call('/login', 'POST', { password: 'openxplay' });
  expect((await call('/password/reset', 'POST')).status).toBe(422);
  await call('/setup/complete', 'POST'); await call('/password/reset', 'POST', { confirmed: true });
  expect((await call('/status')).status).toBe(401);
  await call('/login', 'POST', { password: 'openxplay' });
  expect((await call('/status')).data.setupCompleted).toBe(true);
  expect((await call('/access', 'PUT', { enabled: false })).status).toBe(422);
  await call('/access', 'PUT', { enabled: false, confirmed: true });
  expect((await call('/status')).status).toBe(403); expect((await call('/session')).data.adminAccessAllowed).toBe(false);
});
it('provides empty and error feature pages with matching response metadata', async () => {
  vi.stubEnv('DIRELAY_PREVIEW_STATES', '1');
  try {
    const empty = relay(true, 'feature-empty'); expect((await empty('/logs/export', 'POST', { range: '10m' })).data.empty).toBe(true);
    expect((await empty('/carlink-trace')).data.files).toEqual([]); expect((await empty('/vehicle')).data.reports).toEqual([]);
    const failed = relay(true, 'feature-error'); for (const path of ['/vehicle', '/carlink-trace', '/config/export?kind=user']) expect((await failed(path)).status).toBe(503);
  } finally { vi.unstubAllEnvs(); }
});

it.each(['pairing', 'fresh'])('reports %s with no paired phones without breaking the preview server', async state => {
  vi.stubEnv('DIRELAY_PREVIEW_STATES', '1');
  try { const call = relay(true, state); const status = (await call('/status')).data; expect(status.iphone.name).toBeNull(); expect(status.pathInput.iphone.pick).toBe('NONE'); expect((await call('/phones')).data).toEqual([]); }
  finally { vi.unstubAllEnvs(); }
});
