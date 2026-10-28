// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-27

import { afterEach, expect, it, vi } from 'vitest';
import { vehiclePanel, vehicleState, mountVehicle, type VehicleView } from '../src/vehicle';
import { translate } from '../src/i18n';
vi.mock('../src/dom', () => ({ patchMarkup: (el: { innerHTML: string }, html: string) => { el.innerHTML = html; }, confirmSheet: vi.fn().mockResolvedValue(false) }));
import { confirmSheet } from '../src/dom';
const view: VehicleView = { enabled: false, sensorActive: false, malformedFrames: 0, storageFailed: false, reports: [], history: [] };
afterEach(() => { vi.useRealTimers(); vi.unstubAllGlobals(); vi.clearAllMocks(); });
it.each(['en', 'zh-CN'] as const)('shows off, waiting, raw signals, bounded inspectable history and privacy in %s', language => {
  const t = (key: string, values?: Record<string, string | number>) => translate(language, key, values);
  expect(vehicleState(view)).toBe('off'); expect(vehiclePanel(view, t)).toContain(t('vehicle.privacy'));
  expect(vehicleState({ ...view, enabled: true })).toBe('waiting');
  const reports = [{ kind: 'Gps', receivedAt: 0, fields: { latitude: '<script>', timestamp: '18446744073709551615', speed: 12 } }];
  const data = { ...view, enabled: true, reports, storageFailed: true, history: Array.from({ length: 31 }, (_, id) => ({ id, time: id, reports })) };
  const panel = vehiclePanel(data, t, 10_000);
  expect(vehicleState(data)).toBe('data');
  expect(vehiclePanel({ ...data, sample: true }, t)).toContain(t('vehicle.sample')); expect(panel).toContain('18446744073709551615');
  expect(panel).toContain('&lt;script&gt;'); expect(panel).not.toContain('<script>');
  expect(panel.match(/data-snapshot=/g)).toHaveLength(30); expect(panel).toContain(t('vehicle.storage_error'));
  expect(panel).toContain(t('vehicle.units')); expect(panel).toContain(t('vehicle.history_hint'));
  expect(panel).toContain(t('vehicle.updated', { seconds: 10 })); expect(panel).not.toContain('km/h');
});
it('requires confirmation to clear, sends opt-in settings once and disposes polling', async () => {
  vi.useFakeTimers();
  let click!: (event: unknown) => Promise<void>;
  const content = { innerHTML: '', querySelectorAll: () => [], addEventListener: (_: string, fn: typeof click) => { click = fn; } };
  const error = { textContent: '' };
  const container = { insertAdjacentHTML: vi.fn(), querySelector: (selector: string) => selector.includes('content') ? content : error };
  const fetch = vi.fn(async (url: string) => new Response(JSON.stringify(url.endsWith('/vehicle') ? view : { ok: true })));
  vi.stubGlobal('fetch', fetch);
  const dispose = mountVehicle(container as unknown as HTMLElement, key => key, () => 'error.request');
  await vi.advanceTimersByTimeAsync(0);
  const event = (clear: boolean) => ({ target: { closest: () => ({ hasAttribute: (key: string) => key === (clear ? 'data-vehicle-clear' : 'data-vehicle-enable'), disabled: false }) } });
  await click(event(true)); expect(fetch.mock.calls.some(([url]) => url.includes('clear-vehicle'))).toBe(false);
  vi.mocked(confirmSheet).mockResolvedValueOnce(true); await click(event(true));
  expect(fetch.mock.calls.filter(([url]) => url.includes('clear-vehicle'))).toHaveLength(1);
  await click(event(false)); expect(fetch.mock.calls.some(([url, ...rest]) => url.endsWith('/settings'))).toBe(true);
  dispose(); const count = fetch.mock.calls.length; await vi.advanceTimersByTimeAsync(6000); expect(fetch).toHaveBeenCalledTimes(count);
});
