// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-15

import { afterEach, expect, it, vi } from 'vitest';
import { api, ApiError, NetworkError, events, request } from '../src/api';
afterEach(() => vi.unstubAllGlobals());
function respond(data: unknown, status = 200, headers = {}) {
  const fetcher = vi.fn().mockResolvedValue(new Response(JSON.stringify(data), { status, headers })); vi.stubGlobal('fetch', fetcher); return fetcher;
}
it('uses the API prefix and cookie credentials', async () => {
  const fetcher = respond({ state: 'login', defaultPassword: true }); expect(await api.session()).toEqual({ state: 'login', defaultPassword: true });
  expect(fetcher).toHaveBeenCalledWith('/api/v0/session', expect.objectContaining({ method: 'GET', credentials: 'same-origin', cache: 'no-store' }));
});
it.each(['POST', 'PUT', 'DELETE'])('sends JSON and the CSRF header on %s', async method => {
  const fetcher = respond({ ok: true }); await request('/test', method, { foo: true });
  expect(fetcher).toHaveBeenCalledWith('/api/v0/test', expect.objectContaining({ method, headers: { 'Content-Type': 'application/json', 'X-OpenXPlay-Admin': '1' }, body: '{"foo":true}' }));
});
it('sends an empty JSON object for bodyless mutations', async () => { const fetcher = respond({ ok: true }); await api.logout(); expect(fetcher.mock.calls[0][1].body).toBe('{}'); });
it('emits unauthorized on 401 and retains the error key', async () => {
  respond({ error: 'password' }, 401); const listener = vi.fn(); events.addEventListener('unauthorized', listener, { once: true });
  await expect(api.login('openxplay')).rejects.toMatchObject({ status: 401, key: 'password' }); expect(listener).toHaveBeenCalledOnce();
});
it('retains individual field validation errors', async () => {
  respond({ errors: { wifiChannel: 'error.range' } }, 422); await expect(api.save({ wifiChannel: 999 })).rejects.toMatchObject({ status: 422, errors: { wifiChannel: 'error.range' } });
});
it('retains action conflicts', async () => { respond({ error: 'error.conflict' }, 409); await expect(api.action('start')).rejects.toMatchObject({ status: 409, key: 'error.conflict' }); });
it('reports 429 retry timing', async () => { respond({ error: 'rate' }, 429, { 'Retry-After': '25' }); await expect(api.login('openxplay')).rejects.toMatchObject({ status: 429, retryAfter: 25, key: 'error.rateLimit' }); });
it('reports network failures and reconnect events', async () => {
  vi.stubGlobal('fetch', vi.fn().mockRejectedValue(new TypeError('offline'))); const listener = vi.fn(); events.addEventListener('offline', listener, { once: true });
  await expect(api.status()).rejects.toBeInstanceOf(NetworkError); expect(listener).toHaveBeenCalledOnce();
  const online = vi.fn(); events.addEventListener('online', online, { once: true }); respond({}); await api.status(); expect(online).toHaveBeenCalledOnce();
});
it('handles non-JSON error pages', async () => { vi.stubGlobal('fetch', vi.fn().mockResolvedValue(new Response('Bad gateway', { status: 502 }))); await expect(api.status()).rejects.toBeInstanceOf(ApiError); });
it('retries a transient network failure once and returns the recovered result', async () => {
  const fetcher = vi.fn().mockRejectedValueOnce(new TypeError('offline')).mockResolvedValueOnce(new Response('{"state":"ready"}'));
  vi.stubGlobal('fetch', fetcher);
  expect(await api.session()).toEqual({ state: 'ready' }); expect(fetcher).toHaveBeenCalledTimes(2);
});

it('sends password login and password change bodies', async () => {
  const fetcher = respond({ ok: true }); await api.login('openxplay'); await api.password('openxplay', 'updated password');
  expect(JSON.parse(fetcher.mock.calls[0][1].body)).toEqual({ password: 'openxplay' });
  expect(JSON.parse(fetcher.mock.calls[1][1].body)).toEqual({ current: 'openxplay', new: 'updated password' });
});
it('does not retry a Wi-Fi handoff after losing the response', async () => {
  const fetcher = vi.fn().mockRejectedValue(new TypeError('offline')); vi.stubGlobal('fetch', fetcher);
  await expect(api.wifi('Home Wi-Fi', 'secret')).rejects.toBeInstanceOf(NetworkError);
  expect(fetcher).toHaveBeenCalledOnce();
  expect(fetcher.mock.calls[0][0]).toBe('/api/v0/wifi');
});
it('does not retry forgetting Wi-Fi', async () => {
  const fetcher = vi.fn().mockRejectedValue(new TypeError('offline')); vi.stubGlobal('fetch', fetcher);
  await expect(api.action('forget-wifi')).rejects.toBeInstanceOf(NetworkError); expect(fetcher).toHaveBeenCalledOnce();
});
it('submits a Carlink PIN once with leading zeroes intact', async () => {
  const fetcher = respond({ ok: true }); await api.carlinkPin('012345');
  expect(fetcher).toHaveBeenCalledWith('/api/v0/carlink/pin', expect.objectContaining({ method: 'POST', body: '{"pin":"012345"}' }));
  fetcher.mockRejectedValue(new TypeError('offline')); fetcher.mockClear();
  await expect(api.carlinkPin('012345')).rejects.toBeInstanceOf(NetworkError); expect(fetcher).toHaveBeenCalledOnce();
});
it('does not retry forgetting a Carlink head unit', async () => {
  const fetcher = vi.fn().mockRejectedValue(new TypeError('offline')); vi.stubGlobal('fetch', fetcher);
  await expect(api.action('forget-carlink')).rejects.toBeInstanceOf(NetworkError); expect(fetcher).toHaveBeenCalledOnce();
  expect(fetcher.mock.calls[0][0]).toBe('/api/v0/actions/forget-carlink');
});

it('connects with an optional pre-armed PIN and never retries an attempt', async () => {
  const fetcher = respond({ ok: true });
  await api.carlinkConnect('012345'); await api.carlinkConnect();
  expect(fetcher.mock.calls[0][0]).toBe('/api/v0/carlink/connect');
  expect(fetcher.mock.calls[0][1].body).toBe('{"pin":"012345"}');
  expect(fetcher.mock.calls[1][1].body).toBe('{}');
  fetcher.mockRejectedValue(new TypeError('offline')); fetcher.mockClear();
  await expect(api.carlinkConnect('012345')).rejects.toBeInstanceOf(NetworkError);
  expect(fetcher).toHaveBeenCalledOnce();
});

it('omits a Wi-Fi password to connect with saved credentials and preserves explicit open passwords', async () => {
  const fetcher = respond({ ok: true }); await api.wifi('Saved'); await api.wifi('Open', '');
  expect(JSON.parse(fetcher.mock.calls[0][1].body)).toEqual({ ssid: 'Saved' });
  expect(JSON.parse(fetcher.mock.calls[1][1].body)).toEqual({ ssid: 'Open', password: '' });
});
it('never retries a mode change after losing its response and retains no-saved-Wi-Fi errors', async () => {
  const fetcher = vi.fn().mockRejectedValue(new TypeError('radio switching')); vi.stubGlobal('fetch', fetcher);
  await expect(api.switchNetwork('device-hotspot')).rejects.toBeInstanceOf(NetworkError);
  expect(fetcher).toHaveBeenCalledOnce();
  expect(fetcher.mock.calls[0]).toEqual(['/api/v0/settings', expect.objectContaining({ method: 'PUT', body: '{"networkMode":"device-hotspot"}' })]);
  respond({ errors: { networkMode: 'error.noSavedWifi' } }, 422);
  await expect(api.switchNetwork('shared-wifi')).rejects.toMatchObject({ errors: { networkMode: 'error.noSavedWifi' } });
});

it('sends hotspot credentials once and never retries output changes', async () => {
  const fetcher = respond({ ok: true });
  await api.hotspot('Relay car', 'newpass123');
  expect(fetcher.mock.calls[0]).toEqual(['/api/v0/hotspot', expect.objectContaining({ method: 'PUT', body: '{"ssid":"Relay car","password":"newpass123"}' })]);
  fetcher.mockRejectedValue(new TypeError('connection lost')); fetcher.mockClear();
  await expect(api.hotspot('Relay car', 'newpass123')).rejects.toBeInstanceOf(NetworkError);
  expect(fetcher).toHaveBeenCalledOnce();
  fetcher.mockClear();
  await expect(api.save({ output: 'carlink' })).rejects.toBeInstanceOf(NetworkError);
  expect(fetcher).toHaveBeenCalledOnce();
});
it('sends an optional Linux hotspot band with the credentials', async () => {
  const fetcher = respond({ ok: true });
  await api.hotspot('Relay car', 'newpass123', '2.4');
  expect(JSON.parse(fetcher.mock.calls[0][1].body)).toEqual({ ssid: 'Relay car', password: 'newpass123', band: '2.4' });
});
