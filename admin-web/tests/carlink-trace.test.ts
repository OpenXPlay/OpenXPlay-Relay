// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-22

import { afterEach, expect, it, vi } from 'vitest';
import { mountTrace, traceFilters, tracePanel, traceVisible, TracePoller, type TraceSnapshot } from '../src/carlink-trace';
import { translate } from '../src/i18n';
import { settingsViews } from '../src/logic';
import { mockSettings } from '../dev-server';
import { readFileSync } from 'node:fs';
const snapshot: TraceSnapshot = { enabled: false, running: false, status: { dropped: 4, failure: 'PermissionDenied', capture: { state: 'idle', bytes: 188 } }, files: [
  { name: 'trace-00000001-0-00000001.jsonl', kind: 'trace', size: 20 },
  { name: 'media-00000001-00000001-0.ts', kind: 'media', size: 188 },
  { name: 'media-00000001-00000002-0.ts.part', kind: 'media-partial', size: 1 },
] };
afterEach(() => { vi.useRealTimers(); vi.unstubAllGlobals(); });
it.each(['en', 'zh-CN'] as const)('renders trace counters, errors, Media and finished downloads in %s', lang => {
  const t = (key: string, p?: Record<string, string | number>) => translate(lang, key, p);
  const html = tracePanel(snapshot, ['<script>'], '{"type":"Media"}', t);
  expect(html).toContain('188'); expect(html).toContain('PermissionDenied'); expect(html).toContain('Media');
  expect(html).toContain('&lt;script&gt;'); expect(html).not.toContain('<script>');
  expect(html).toContain('/api/v0/carlink-trace/download'); expect(html).toContain(snapshot.files[1].name);
  expect(html).not.toContain('.ts.part'); expect(html).toContain('data-trace="capture" disabled');
  expect(html).toContain(t('trace.start'));
  expect(tracePanel({ ...snapshot, enabled: true }, [], undefined, t)).not.toContain('data-trace="capture" disabled');
  const active = { ...snapshot, status: { ...snapshot.status!, capture: { state: 'capturing', bytes: 0 } } };
  const running = tracePanel(active, [], undefined, t);
  expect(running).toContain(t('trace.stop')); expect(running).not.toContain('data-trace="capture" disabled');
  for (const key of ['trace.title', 'field.carlinkTrace', 'trace.clear', 'trace.clearConfirm', 'trace.all', 'trace.rtsp', 'trace.frames', 'trace.media', 'trace.link', 'trace.errors']) expect(t(key)).not.toBe(key);
});
it('is confined to Linux developer settings and preserves Android schema goldens', () => {
  for (const platform of ['linux', 'android']) for (const developer of [true, false]) for (const view of ['home', 'settings']) expect(traceVisible(platform, developer, view)).toBe(platform === 'linux' && developer && view === 'settings');
  const settings = mockSettings(true);
  settings.schema[0].fields.push({ key: 'carlinkTrace', type: 'choice', labelKey: 'field.carlinkTrace' });
  expect(settingsViews(settings, true, 'linux').find(s => s.id === 'developer')?.fields.some(f => f.key === 'carlinkTrace')).toBe(true);
  expect(settingsViews(settings, false, 'linux').flatMap(s => s.fields).some(f => f.key === 'carlinkTrace')).toBe(false);
  expect(mockSettings(false).schema.flatMap(s => s.fields).some(f => f.key === 'carlinkTrace')).toBe(false);
  const source = readFileSync(new URL('../src/main.ts', import.meta.url), 'utf8');
  expect(source).toContain('disposeTrace?.(); disposeTrace = null;');
  expect(source).toContain("route === 'settings/developer/carlink' && status!.platform === 'linux'");
  expect(source).toContain("checked ? 'on' : 'off'");
});
it('polls every two seconds, never overlaps reads and stops even during an in-flight read', async () => {
  vi.useFakeTimers();
  let resolve!: () => void;
  const read = vi.fn().mockResolvedValue(undefined).mockImplementationOnce(() => new Promise<void>(r => { resolve = r; }));
  const poll = new TracePoller(read); poll.start();
  await vi.advanceTimersByTimeAsync(6000); expect(read).toHaveBeenCalledTimes(1);
  resolve(); await vi.advanceTimersByTimeAsync(2000); expect(read).toHaveBeenCalledTimes(2);
  poll.dispose(); await vi.advanceTimersByTimeAsync(6000); expect(read).toHaveBeenCalledTimes(2);
  const second = new TracePoller(() => new Promise<void>(r => { resolve = r; }));
  second.start(); second.dispose(); resolve(); await vi.advanceTimersByTimeAsync(6000); expect(vi.getTimerCount()).toBe(0);
});
it('uses exact Link and Error filters and stops fetching after the card is disposed', async () => {
  vi.useFakeTimers();
  const select = { value: '', addEventListener: vi.fn() };
  const content = { innerHTML: '', ownerDocument: { activeElement: null }, querySelector: () => null, querySelectorAll: () => [], addEventListener: vi.fn() };
  const error = { textContent: '' };
  const card = { querySelector: (selector: string) => selector.includes('filter') ? select : selector.includes('content') ? content : error };
  const fetch = vi.fn(async (url: string) => new Response(JSON.stringify(url.endsWith('carlink-trace') ? snapshot : { lines: ['{"type":"Link"}'] })));
  vi.stubGlobal('fetch', fetch);
  const dispose = mountTrace(card as unknown as HTMLElement, key => key, () => 'error.request', (node, html) => { node.innerHTML = html; });
  await vi.advanceTimersByTimeAsync(0); expect(content.innerHTML).toContain('Link');
  for (const type of ['Link', 'Error']) {
    select.value = type; select.addEventListener.mock.calls[0][1](); await vi.advanceTimersByTimeAsync(0);
    expect(fetch.mock.calls.some(([url]) => url.endsWith(`limit=5000&type=${type}`))).toBe(true);
  }
  expect(traceFilters).toEqual(['', 'Rtsp', 'Frame', 'Media', 'Link', 'Error']);
  dispose(); const count = fetch.mock.calls.length;
  await vi.advanceTimersByTimeAsync(6000); expect(fetch).toHaveBeenCalledTimes(count);
});

it.each([0, 180])('preserves trace scrollTop %s and focused button across polling renders', async scrollTop => {
  vi.useFakeTimers();
  const ownerDocument = { activeElement: null as unknown };
  const select = { addEventListener: vi.fn() };
  let list = { scrollTop: 0 };
  const button = () => ({ dataset: { trace: 'capture' }, disabled: false, focus: vi.fn(function (this: unknown) { ownerDocument.activeElement = this; }) });
  let capture = button();
  const content = {
    ownerDocument,
    set innerHTML(_html: string) { list = { scrollTop: 0 }; capture = button(); ownerDocument.activeElement = null; },
    querySelector: () => list, querySelectorAll: () => [capture], addEventListener: vi.fn(),
  };
  const card = { querySelector: (selector: string) => selector.includes('filter') ? select : selector.includes('content') ? content : { textContent: '' } };
  vi.stubGlobal('fetch', vi.fn(async (url: string) => new Response(JSON.stringify(url.endsWith('carlink-trace') ? { ...snapshot, enabled: true } : { lines: ['{"type":"Link"}'] }))));
  const dispose = mountTrace(card as unknown as HTMLElement, key => key, () => 'error.request', (node, html) => { node.innerHTML = html; });
  await vi.advanceTimersByTimeAsync(0);
  list.scrollTop = scrollTop; ownerDocument.activeElement = capture;
  const previous = capture;
  await vi.advanceTimersByTimeAsync(2000);
  expect(capture).not.toBe(previous);
  expect(ownerDocument.activeElement).toBe(capture);
  expect(capture.focus).toHaveBeenCalledWith({ preventScroll: true });
  expect(list.scrollTop).toBe(scrollTop);
  ownerDocument.activeElement = select;
  await vi.advanceTimersByTimeAsync(2000);
  expect(capture.focus).not.toHaveBeenCalled();
  dispose();
});
