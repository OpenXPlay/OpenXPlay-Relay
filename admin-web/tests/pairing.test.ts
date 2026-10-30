// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-16

import { afterEach, expect, it, vi } from 'vitest';
import { anchorPairing, createCountdown, formatRemaining, pairingPanelView, pairingView, relayBluetoothName } from '../src/pairing';
import { statusView } from '../src/views';
import { translate } from '../src/i18n';
import { previewStatus } from '../dev-server';
import type { Status } from '../src/api';
const en = (key: string, parameters?: Record<string, string | number>) => translate('en', key, parameters);
const zh = (key: string, parameters?: Record<string, string | number>) => translate('zh-CN', key, parameters);
const fixture = (state = 'working', platform: Status['platform'] = 'linux') => previewStatus({ platform, host: 'openxplay-linux.local', receiverUrls: [] } as unknown as Status, state);
afterEach(() => { vi.useRealTimers(); });

it('anchors the relay countdown on the browser clock without jitter', () => {
  const status = fixture(); status.pairingDeadline = 999999; status.pairingRemaining = 581;
  expect(anchorPairing(status, 1000.4).pairingDeadline).toBe(1581);
  expect(anchorPairing(status, 1000.4, 1582).pairingDeadline).toBe(1582);
  expect(anchorPairing(status, 1000.4, 1590).pairingDeadline).toBe(1581);
  expect(anchorPairing({ ...status, pairingRemaining: 0 }, 1000).pairingDeadline).toBeNull();
  expect(anchorPairing({ ...status, pairingRemaining: undefined }, 1000).pairingDeadline).toBe(999999);
});
it('formats the remaining time as minutes and seconds', () => {
  expect(formatRemaining(600)).toBe('10:00');
  expect(formatRemaining(581)).toBe('9:41');
  expect(formatRemaining(59.2)).toBe('1:00');
  expect(formatRemaining(-3)).toBe('0:00');
});
it('models the window, cancel support, confirmation and paired devices', () => {
  const status = fixture(); status.pairingDeadline = 1600;
  status.pairedDevices = [{ name: 'Car audio', connected: false, iphone: false }, { name: 'Spare iPhone', connected: false, iphone: true }, { name: "Riko's iPhone", connected: true, iphone: true }];
  const open = pairingView(status, 1000);
  expect(open).toMatchObject({ open: true, seconds: 600, time: '10:00', cancelable: true, confirm: false });
  expect(open.devices.map(d => d.name)).toEqual(["Riko's iPhone", 'Spare iPhone', 'Car audio']);
  expect(pairingView({ ...status, pairingCancelable: false }, 1000).cancelable).toBe(false);
  expect(pairingView({ ...status, pairingDeadline: null, pairingNeedsConfirmation: true }, 1000)).toMatchObject({ open: false, confirm: true });
  expect(pairingView({ ...status, pairingDeadline: 900 }, 1000)).toMatchObject({ open: false, seconds: 0, cancelable: false });
});
it('renders the closed panel with one explanation sentence and the 10-minute button', () => {
  const html = pairingPanelView(fixture(), false, '', en, 1000);
  expect(html).toContain('Use this when pairing a new iPhone or after the iPhone forgot this relay.');
  expect(html).toContain('id="pairing-open"');
  expect(html).toContain('Open pairing for 10 minutes');
  expect(html).not.toContain('pairing-cancel');
  expect(html).toContain("Riko&#39;s iPhone");
  expect(html.match(/class="tag"/g)).toHaveLength(1);
  expect(pairingPanelView(fixture(), true, '', en, 1000)).toMatch(/id="pairing-open" disabled/);
});
it('renders the open Linux window with the countdown and a cancel button', () => {
  const status = fixture(); status.pairingDeadline = 1581;
  const html = pairingPanelView(status, false, '', en, 1000);
  expect(html).toContain('<p id="pairing-remaining" role="timer">Pairing is open · 9:41 left</p>');
  expect(html).toContain('id="pairing-cancel"');
  expect(html).not.toContain('pairing-open');
  expect(pairingPanelView(status, false, '', zh, 1000)).toContain('配对已开放 · 剩余 9:41');
});
it('names the relay as the iPhone lists it under Bluetooth', () => {
  const linux = { ...fixture(), name: 'OpenXPlay Linux', pathInput: { relay: { bluetoothName: 'OpenXPlay Linux' } }, pairingDeadline: 1581 } as Status;
  expect(pairingPanelView(linux, false, '', en, 1000)).toContain('Bluetooth and choose OpenXPlay Linux.');
  vi.useFakeTimers(); vi.setSystemTime(1_000_000);
  expect(statusView(linux, en)).toContain('choose OpenXPlay Linux. Pairing closes in 9:41.');
  expect(pairingPanelView(linux, false, '', zh, 1000)).toContain('选择 OpenXPlay Linux。');
  const android = { ...fixture('working', 'android'), pathInput: { relay: { bluetoothName: 'Riko <Pixel>' } }, pairingDeadline: 1300 } as Status;
  expect(pairingPanelView(android, false, '', en, 1000)).toContain('choose Riko &lt;Pixel&gt;.');
  expect(relayBluetoothName({ ...linux, pathInput: undefined })).toBe('OpenXPlay Linux');
});
it('explains Android windows that cannot be cancelled and the confirmation on the relay screen', () => {
  const status = fixture('working', 'android'); status.pairingDeadline = 1300;
  const open = pairingPanelView(status, false, '', en, 1000);
  expect(open).not.toContain('pairing-cancel');
  expect(open).toContain('The relay stays visible until the time runs out.');
  const waiting = pairingPanelView({ ...status, pairingDeadline: null, pairingNeedsConfirmation: true }, false, '', en, 1000);
  expect(waiting).toContain('Tap Allow on the relay’s screen');
  expect(waiting).not.toContain('pairing-open');
});
it('reports the Android 12+ Nearby devices step, the notification and a declined permission', () => {
  const status = fixture('working', 'android');
  const permission = pairingPanelView({ ...status, pairingNeedsPermission: true }, false, '', en, 1000);
  expect(permission).toContain('Allow “Nearby devices” on the relay’s screen');
  expect(permission).not.toContain('pairing-open');
  const notified = { ...status, pairingNeedsPermission: true, pairingNotified: true };
  expect(pairingView(notified, 1000)).toMatchObject({ open: false, permission: true, notified: true });
  expect(pairingPanelView(notified, false, '', en, 1000)).toContain('Tap the OpenXPlay Relay notification on the relay’s screen');
  expect(pairingPanelView(notified, false, '', zh, 1000)).toContain('通知');
  const denied = pairingPanelView({ ...status, pairingOnRelay: true, pairingPermissionDenied: true }, false, '', en, 1000);
  expect(denied).toContain('Bluetooth settings are open on the relay’s screen');
  expect(denied).toContain('“Nearby devices” was not allowed');
  expect(pairingPanelView({ ...status, pairingOnRelay: true }, false, '', en, 1000)).not.toContain('was not allowed');
  expect(pairingView({ ...status, pairingDeadline: 1300, pairingNeedsPermission: true }, 1000)).toMatchObject({ open: true, permission: false });
  const hero = statusView({ ...status, pairingNotified: true, pairingNeedsConfirmation: true }, en);
  expect(hero).toContain('Tap the OpenXPlay Relay notification');
  expect(hero).not.toContain('Tap Allow on the relay’s screen');
  expect(pairingPanelView(status, false, 'error.pairingOpenApp', en, 1000)).toContain('Open OpenXPlay Relay on the relay’s screen');
});
it('escapes device names, reports errors and lists an empty state', () => {
  const status = fixture(); status.pairedDevices = [{ name: '<img src=x>', connected: false, iphone: true }, { name: '', connected: false, iphone: true }];
  const html = pairingPanelView(status, false, 'error.pairing', en, 1000);
  expect(html).toContain('&lt;img src=x&gt;');
  expect(html).not.toContain('<img');
  expect(html).toContain('Unnamed device');
  expect(html).toContain('Bluetooth pairing could not start.');
  expect(pairingPanelView({ ...status, pairedDevices: [] }, false, '', en, 1000)).toContain('No paired devices yet.');
  expect(pairingPanelView({ ...status, pairingAvailable: false }, false, '', en, 1000)).toContain('Open Bluetooth settings on the relay device');
});
it('ticks every second, expires once and stops', () => {
  vi.useFakeTimers(); vi.setSystemTime(1_000_000);
  let deadline: number | null = 1003;
  const seen: number[] = []; const expired = vi.fn();
  const countdown = createCountdown(() => deadline, seconds => seen.push(seconds), expired);
  countdown.start(); countdown.start();
  expect(seen).toEqual([3]);
  vi.advanceTimersByTime(3000);
  expect(seen).toEqual([3, 2, 1, 0]);
  expect(expired).toHaveBeenCalledOnce();
  vi.advanceTimersByTime(2000);
  expect(expired).toHaveBeenCalledOnce();
  deadline = 1006 + 600;
  vi.advanceTimersByTime(1000);
  expect(seen.at(-1)).toBe(600);
  countdown.stop();
  vi.advanceTimersByTime(5000);
  expect(seen.at(-1)).toBe(600);
});
it('never expires a window that was never open', () => {
  vi.useFakeTimers();
  const expired = vi.fn();
  createCountdown(() => null, () => {}, expired).start();
  vi.advanceTimersByTime(5000);
  expect(expired).not.toHaveBeenCalled();
});
it('shows the Home countdown in minutes and seconds', () => {
  vi.useFakeTimers(); vi.setSystemTime(1_000_000);
  const status = fixture('no-iphone'); status.pairingDeadline = 1000 + 581;
  expect(statusView(status, en)).toContain('Pairing closes in 9:41.');
  expect(statusView(status, zh)).toContain('配对将在 9:41 后关闭');
  expect(statusView({ ...status, pairingDeadline: null, pairingNeedsConfirmation: true }, en)).toContain('Tap Allow on the relay’s screen');
});
it('tells the user when Android opened Bluetooth settings on the relay instead of a timed window', () => {
  const status = fixture('no-iphone', 'android'); status.pairingDeadline = null; status.pairingOnRelay = true;
  expect(pairingView(status, 1000)).toMatchObject({ open: false, onRelay: true, confirm: false });
  const html = pairingPanelView(status, false, '', en, 1000);
  expect(html).toContain('Bluetooth settings are open on the relay’s screen');
  expect(html).not.toContain('pairing-open');
  expect(pairingPanelView(status, false, '', zh, 1000)).toContain('中继屏幕上已打开蓝牙设置');
  expect(statusView(status, en)).toContain('Bluetooth settings are open on the relay’s screen');
});

it('hides non-iPhones and suppresses the duplicate paired list in the Home drawer', () => {
  const status = fixture(); status.pairedDevices = [{ name: 'Car audio', connected: true, iphone: false }, { name: 'My iPhone', connected: true, iphone: true }];
  expect(pairingPanelView(status, false, '', en)).not.toContain('Car audio');
  expect(pairingPanelView(status, false, '', en, undefined, false)).not.toContain('My iPhone');
});
