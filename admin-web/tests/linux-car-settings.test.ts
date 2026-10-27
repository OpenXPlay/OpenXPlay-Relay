// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-21

import { expect, it } from 'vitest';
import { readFileSync } from 'node:fs';
import { mockSettings, mockReceivers, previewStatus } from '../dev-server';
import { carlinkPinError, carlinkView, homeView, settingsViews } from '../src/logic';
import { isVisible } from '../src/schema';
import { devicesView, statusView } from '../src/views';
import { diagnosticsView } from '../src/diagnostics';
import { VideoStatsSnapshot, videoStatsInterval, videoStatsView } from '../src/video-stats';
import { translate } from '../src/i18n';
import type { Status } from '../src/api';
const t = (key: string, parameters?: Record<string, string | number>) => translate('en', key, parameters);
const fixture = () => ({ ...previewStatus({ host: 'relay', receiverUrls: [] } as unknown as Status, 'working'), platform: 'linux' as const, receivers: mockReceivers() });
it('places Linux Carlink settings in Car screen, startup in connection, and technical controls in developer', () => {
  const settings = mockSettings(true);
  const sections = settingsViews(settings, true, 'linux');
  const keys = (id: string) => sections.find(s => s.id === id)!.fields.map(f => f.key);
  expect(keys('connection')).toContain('autoStart');
  expect(keys('output')).toEqual(expect.arrayContaining(['carlinkName', 'carlinkAutoReconnect', 'carlinkReconnectOnResize', 'videoStats', 'carPlayViewAreas']));
  expect(keys('developer')).toEqual(expect.arrayContaining(['carlinkModel', 'carlinkHevc', 'receiverPort']));
  expect(settingsViews(settings, false, 'linux').flatMap(s => s.fields).map(f => f.key)).not.toEqual(expect.arrayContaining(['carlinkModel', 'carlinkHevc', 'receiverPort']));
  for (const key of ['carlinkName', 'carlinkAutoReconnect', 'carlinkReconnectOnResize']) {
    const field = settings.schema.flatMap(s => s.fields).find(f => f.key === key)!;
    expect(isVisible(field, { output: 'web' })).toBe(false);
    for (const output of ['carlink', 'both']) expect(isVisible(field, { output })).toBe(true);
  }
});
it.each(['5', '15'])('refreshes the session statistics card every %s seconds', interval => {
  const settings = mockSettings(true); settings.values.videoStats = interval;
  const ms = Number(interval) * 1000;
  expect(videoStatsInterval(settings, 'linux')).toBe(ms);
  const snapshot = new VideoStatsSnapshot(); const status = fixture();
  status.carlink = { state: 'playing', detail: '', viewSize: { width: 1814, height: 1200 }, stats: { liveFrames: 1, liveBytes: 100, codec: 'H265' } };
  const first = snapshot.read(status, ms, 0);
  const html = videoStatsView(first, t);
  expect(html).toContain('Video statistics'); expect(html).toContain('1814 × 1200'); expect(html).toContain('Live frames'); expect(html).toContain('H265'); expect(html).toContain('fps');
  status.carlink.stats!.liveFrames = 20;
  expect(snapshot.read(status, ms, ms - 1)?.carlink?.stats?.liveFrames).toBe(1);
  expect(snapshot.read(status, ms, ms)?.carlink?.stats?.liveFrames).toBe(20);
  expect(snapshot.read(status, 0, ms)).toBe(null);
  expect(videoStatsView(null, t)).toBe('');
});
it('hides statistics when off, without a schema field, on Android, and between sessions', () => {
  const settings = mockSettings(true);
  expect(videoStatsInterval(settings, 'linux')).toBe(0);
  settings.values.videoStats = '5';
  expect(videoStatsInterval(settings, 'android')).toBe(0);
  settings.schema = [];
  expect(videoStatsInterval(settings, 'linux')).toBe(0);
  const status = fixture(); const snapshot = new VideoStatsSnapshot();
  snapshot.read(status, 15000, 0);
  status.state = 'idle'; status.carlink = null;
  expect(snapshot.read(status, 15000, 1)).toBe(null);
  status.state = 'active'; status.receivers = [];
  expect(snapshot.read(status, 15000, 2)?.receivers).toEqual([]);
});
it('clears cached statistics when sessions or authenticated users change', () => {
  const status = fixture(); const snapshot = new VideoStatsSnapshot();
  status.carlink = { state: 'playing', detail: '', stats: { liveFrames: 500 } };
  snapshot.read(status, 15000, 0);
  status.carlink.stats = { liveFrames: 1 };
  expect(snapshot.read(status, 15000, 1)?.carlink?.stats?.liveFrames).toBe(1);
  status.carlink.stats = null;
  expect(snapshot.read(status, 15000, 2)?.carlink?.stats).toBe(null);
  snapshot.reset(); status.carlink.stats = { liveFrames: 20 };
  expect(snapshot.read(status, 15000, 3)?.carlink?.stats?.liveFrames).toBe(20);
});
it('shows Carlink view size and all media counters on Home and in developer diagnostics', () => {
  const status = fixture();
  status.carlink = { state: 'playing', detail: '', viewSize: { width: 1814, height: 1200 }, stats: { sent: 5, dropped: 4, liveFrames: 2, placeholderFrames: 3, droppedFrames: 4, liveBytes: 5678, sendStalls: 6, codec: 'H265', width: 1450, height: 960 } };
  const html = devicesView(status, false, t);
  expect(html).toContain('1814 × 1200'); expect(html).toContain('5678'); expect(html).toContain('Send stalls');
  const diagnostic = diagnosticsView(status, t);
  expect(diagnostic).toContain('viewSize'); expect(diagnostic).toContain('liveFrames'); expect(diagnostic).toContain('sendStalls');
});
it('shows the wrong PIN message while allowing the next PIN request', () => {
  const status = fixture(); status.output = 'carlink';
  status.carlink = { state: 'authenticating', detail: '', pinRequested: true, pinIncorrect: true, pinDeadline: 120 };
  expect(carlinkView(status, 10).pinRequested).toBe(true);
  expect(t(carlinkPinError(status.carlink))).toBe('Wrong PIN. Check the code on the car screen and try again.');
  expect(translate('zh-CN', carlinkPinError(status.carlink))).toBe('PIN 码错误，请核对车机屏幕上的代码后重试。');
  status.carlink.pinIncorrect = false;
  expect(carlinkPinError(status.carlink)).toBe('');
  const source = readFileSync(new URL('../src/main.ts', import.meta.url), 'utf8');
  expect(source).toContain("dialog.querySelector('#carlink-pin-error')!.textContent = t(carlinkPinError(status?.carlink))");
  expect(source).toContain("api.carlinkPin(input.value)");
});
it('offers Connect for a trusted paused head unit and Start when Linux auto-start is paused', () => {
  const status = fixture(); status.output = 'carlink';
  status.carlink = { trusted: true, state: 'paused', detail: 'carlink.manualHint' };
  expect(homeView(status).action).toBe('car');
  status.autoStartPaused = true;
  expect(statusView(status, t)).toContain('id="home-start"');
});
it('leaves Android-shaped schemas and status output unchanged', () => {
  const settings = mockSettings(false);
  expect(settingsViews(settings, false, 'android').map(s => [s.id, s.fields.map(f => f.key)])).toEqual([
    ['connection', ['iphone', 'networkMode', 'sharedWifiNetworks', 'autoStart', 'bootStart']],
    ['output', ['output', 'requirePairingCode', 'wirelessDisplay', 'carlinkName', 'vehicleInformation', 'carBluetoothAudio', 'carlinkAutoReconnect', 'plugins', 'pluginsEnabled']],
  ]);
  const status = fixture(); status.platform = 'android'; status.autoStartPaused = true;
  expect(statusView(status, t)).not.toContain('home-start');
  expect(devicesView(status, false, t)).not.toContain('Live frames');
  expect(videoStatsInterval(settings, status.platform)).toBe(0);
  expect(settings.schema.flatMap(s => s.fields).some(f => ['carlinkModel', 'carlinkHevc', 'receiverPort', 'videoStats', 'carPlayViewAreas', 'carlinkReconnectOnResize'].includes(f.key))).toBe(false);
});

it('explains the Linux HEVC policy in both languages and allows low receiver ports', () => {
  const fields = mockSettings(true).schema.flatMap(s => s.fields);
  expect(fields.find(f => f.key === 'carlinkHevc')?.hintKey).toBe('hint.field.carlinkHevc');
  expect(fields.find(f => f.key === 'receiverPort')?.min).toBe(1);
  expect(translate('en', 'hint.field.carlinkHevc')).toBe('Automatic uses H.265 only when the relay outputs to Carlink alone and the video codec is not forced to H.264.');
  expect(translate('zh-CN', 'hint.field.carlinkHevc')).toContain('H.264');
});
