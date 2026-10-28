// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-27

import { expect, it } from 'vitest';
import { VideoStatsSampler, VideoStatsSnapshot, videoStatsView, videoStatsInterval, type VideoCounters } from '../src/video-stats';
import type { Status } from '../src/api';
import { linuxStatusFixtures, mockSettings } from '../dev-server';
import { translate } from '../src/i18n';
import { diagnosticsView } from '../src/diagnostics';
import { menuSummary } from '../src/settings-tree';
const t = (key: string, parameters?: Record<string, string | number>) => translate('en', key, parameters);
const counters = (frames = 0, bytes = 0, dropped = 0, placeholders = 0, stalls = 0, relayDropped = 0): VideoCounters => ({ frames, bytes, dropped, placeholders, stalls, relayDropped });
const fixture = (): Status => ({ ...structuredClone(linuxStatusFixtures['live-browser']), output: 'both',
  video: { codec: 'H265', reason: 'all-main-screen-outputs-hardware-hevc', session: '1', active: true, frames: 100, bytes: 1000, bytesMeasured: true, width: 1920, height: 720, appliedFps: 60, requestedFps: 55 },
  carlink: { state: 'playing', detail: '', stats: { codec: 'H265', width: 1920, height: 720, liveFrames: 100, liveBytes: 1000, droppedFrames: 5, placeholderFrames: 0, sendStalls: 0 } },
  receivers: [{ client: '1', idleSeconds: 0, info: null, stats: [{ session: '1', screen: 0, rendered: 100, dropped: 5, relayDroppedFrames: 10, decodeMs: 8, renderedLatencyMs: 15 }] }],
});
it('uses actual elapsed time and cumulative deltas rather than arrival snapshots or per-second rates', () => {
  const sampler = new VideoStatsSampler();
  sampler.sample('1', 5000, counters(100, 1000, 5, 7, 2, 10), 0);
  expect(sampler.report(5000)).toEqual({ seconds: 5 });
  sampler.sample('1', 5000, counters(110, 2000, 5, 7, 2, 10), 500);
  expect(sampler.retainedSamples).toBe(1);
  sampler.sample('1', 5000, counters(140, 5000, 6, 7, 2, 11), 2000);
  expect(sampler.report(5000)).toEqual({ seconds: 3 });
  sampler.sample('1', 5000, counters(200, 11000, 10, 9, 5, 15), 5000);
  expect(sampler.report(5000)).toEqual({ fps: 20, bitsPerSecond: 16000, droppedPercent: 1000 / 110, placeholders: 2, frames: 100, sendStalls: 3 });
  sampler.sample('1', 5000, counters(210, 12000, 10, 9, 5, 15), 7000);
  expect(sampler.report(5000)).toMatchObject({ fps: 14 });
});
it('restarts warmup for every backwards counter, session, window, clock, off, and explicit clear', () => {
  const sampler = new VideoStatsSampler(); const full = counters(100, 100, 100, 100, 100, 100);
  for (const key of Object.keys(full) as (keyof VideoCounters)[]) {
    sampler.clear(); sampler.sample('1', 5000, full, 0); sampler.sample('1', 5000, full, 5000);
    sampler.sample('1', 5000, { ...full, [key]: 99 }, 6000);
    expect(sampler.report(5000)).toEqual({ seconds: 5 });
  }
  sampler.sample('2', 5000, full, 7000); expect(sampler.report(5000)).toEqual({ seconds: 5 });
  sampler.sample('2', 15000, full, 8000); expect(sampler.report(15000)).toEqual({ seconds: 15 });
  sampler.sample('2', 15000, full, 0); expect(sampler.retainedSamples).toBe(1);
  sampler.sample('2', 0, full, 1000); expect(sampler.retainedSamples).toBe(0);
  sampler.clear(); expect(sampler.report(5000)).toEqual({ seconds: 5 });
});
it('bounds each series at sixteen samples and handles an idle window without division by zero', () => {
  const sampler = new VideoStatsSampler();
  for (let second = 0; second < 1000; second++) {
    sampler.sample('1', 15000, counters(second), second * 1000);
    expect(sampler.retainedSamples).toBeLessThanOrEqual(16);
  }
  expect(sampler.report(15000)).toMatchObject({ fps: 1, bitsPerSecond: 0, droppedPercent: 0 });
  sampler.sample('2', 5000, counters(), 0); sampler.sample('2', 5000, counters(), 5000);
  expect(sampler.report(5000)).toMatchObject({ fps: 0, bitsPerSecond: 0, droppedPercent: 0 });
});
it.each([5, 15])('measures input, Carlink and browser counters over the last %s seconds with Android wording', seconds => {
  const settings = mockSettings(true); settings.values.videoStats = String(seconds);
  const window = videoStatsInterval(settings, 'linux'); expect(window).toBe(seconds * 1000);
  const snapshot = new VideoStatsSnapshot(), status = fixture();
  const first = snapshot.read(status, window, 0)!;
  expect(videoStatsView(first, t)).toContain(`Measuring… about ${seconds} s`);
  status.video!.frames = 100 + 20 * seconds; status.video!.bytes = 1000 + 200000 * seconds;
  Object.assign(status.carlink!.stats!, { liveFrames: 100 + 20 * seconds, liveBytes: 1000 + 200000 * seconds, droppedFrames: 5 + 2 * seconds, sendStalls: 3 });
  Object.assign(status.receivers![0].stats[0], { rendered: 100 + 20 * seconds, dropped: 5 + seconds, relayDroppedFrames: 10 + seconds });
  const measured = snapshot.read(status, window, window)!;
  expect(measured.iphone).toMatchObject({ fps: 20, bitsPerSecond: 1600000 });
  expect(measured.carlink).toMatchObject({ fps: 20, droppedPercent: 200 / 22, sendStalls: 3 });
  expect(measured.browsers[0].measurement).toMatchObject({ fps: 20, droppedPercent: 200 / 22 });
  const html = videoStatsView(measured, t);
  expect(html).toContain(`Average of the last ${seconds} s`);
  expect(html).toContain('H.265 · 1920 × 720 · 20.0 fps · 1.6 Mbps');
  expect(html).toContain('H.265 passthrough · 1920 × 720 · 20.0 fps · 9.1 % dropped');
  expect(html).toContain('20.0 fps · 9.1 % dropped · decode 8 ms · latency 15 ms');
  expect(html).toContain('Send stalls 3');
  const zh = videoStatsView(measured, (key, parameters) => translate('zh-CN', key, parameters));
  expect(zh).toContain(`近 ${seconds} 秒平均`); expect(zh).toContain('1.6 Mbps');
  expect(snapshot.read(status, 0, window)).toBe(null);
});
it('keeps placeholder activity until live video advances, resets on missing reports and session changes', () => {
  const snapshot = new VideoStatsSnapshot(), status = fixture();
  status.carlink!.stats!.liveFrames = 0; status.carlink!.stats!.placeholderFrames = 1;
  snapshot.read(status, 5000, 0);
  let value = snapshot.read(status, 5000, 5000)!;
  expect(videoStatsView(value, t)).toContain('Waiting for iPhone video (placeholder)');
  status.carlink!.stats!.liveFrames = 100;
  value = snapshot.read(status, 5000, 6000)!; expect(value.placeholderActive).toBe(false);
  status.carlink!.stats!.placeholderFrames = 2;
  value = snapshot.read(status, 5000, 7000)!; expect(value.placeholderActive).toBe(true);
  status.video!.session = '2'; value = snapshot.read(status, 5000, 8000)!;
  expect(value.iphone).toEqual({ seconds: 5 }); expect(value.browsers).toEqual([]);
  status.carlink!.stats = null;
  value = snapshot.read(status, 5000, 9000)!; expect(value.carlink).toEqual({ seconds: 5 }); expect(value.placeholderActive).toBe(false);
  snapshot.reset(); expect(snapshot.read(status, 5000, 10000)!.iphone).toEqual({ seconds: 5 });
  status.video!.active = false; expect(snapshot.read(status, 5000, 11000)).toBe(null);
});
it('uses only four recent main-screen browsers from the current session and forgets disconnected series', () => {
  const snapshot = new VideoStatsSnapshot(), status = fixture();
  const first = status.receivers![0];
  status.receivers = Array.from({ length: 8 }, (_, i) => ({ ...structuredClone(first), client: String(i) }));
  status.receivers[0].idleSeconds = 15;
  status.receivers[1].stats[0].session = '0'; status.receivers[2].stats[0].screen = 1;
  status.receivers[3].stats[0].rendered = NaN;
  expect(snapshot.read(status, 5000, 0)!.browsers).toHaveLength(4);
  expect(snapshot.read(status, 5000, 5000)!.browsers[0].measurement).toMatchObject({ fps: 0 });
  status.receivers = []; expect(snapshot.read(status, 5000, 6000)!.browsers).toEqual([]);
  status.receivers = [first]; expect(snapshot.read(status, 5000, 7000)!.browsers[0].measurement).toEqual({ seconds: 5 });
  status.video!.bytesMeasured = false;
  expect(snapshot.read(status, 5000, 8000)!.iphone).toEqual({ seconds: 5 });
});
it('presents retained diagnostics without turning them into live browsers and resolves Auto FPS', () => {
  const status = fixture(); status.receivers = [];
  status.receiverHistory = { screens: [{ width: 1920, height: 720, densityDpi: 160, info: { userAgent: '<script>' } }], fullscreenExits: [{ time: 1000, client: '1', report: { autoRestore: 'denied' } }] };
  const html = diagnosticsView(status, t);
  expect(html).toContain('Detected screen data'); expect(html).toContain('Recent involuntary fullscreen exits');
  expect(html).toContain('&lt;script&gt;'); expect(html).toContain('denied');
  const settings = mockSettings(true); settings.values.frameRate = 'auto';
  expect(menuSummary(settings, status, 'settings/picture', 'en', t)).toContain('Auto · 60 fps');
  status.running = false; status.video!.active = false;
  expect(menuSummary(settings, status, 'settings/picture', 'en', t)).toContain('Auto · 55 fps');
});
