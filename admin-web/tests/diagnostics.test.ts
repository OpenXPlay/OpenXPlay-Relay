// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-16

import { expect, it, vi } from 'vitest';
import { browserLabel, codecSupport, copyText, diagnosticsView, gpuLabel, liveLine, liveStats, receiverReport, receiverSummary, relativeTimestamp, videoLine, videoView, windowLine } from '../src/diagnostics';
import { addressesView, devicesView, statusView } from '../src/views';
import { homeView } from '../src/logic';
import { translate } from '../src/i18n';
import { mockReceivers, previewStatus } from '../dev-server';
import type { ReceiverReport, Status } from '../src/api';
const en = (key: string, parameters?: Record<string, string | number>) => translate('en', key, parameters);
const zh = (key: string, parameters?: Record<string, string | number>) => translate('zh-CN', key, parameters);
const fixture = () => previewStatus({ platform: 'linux', host: 'openxplay-linux.local', receiverUrls: [] } as unknown as Status, 'working');
// Mirrors spec/vectors/web/receiver-info.json from OpenXPlay-Stream.
const vector = { type: 'receiver-info', userAgent: 'ExampleBrowser/1.0', videoDecoder: true,
  videoDecode: [{ codec: 'avc1.42E01F', width: 1280, height: 720, preferHardware: true, preferSoftware: true, fps30: { supported: true, smooth: true, powerEfficient: true }, fps60: { supported: true, smooth: false, powerEfficient: true }, mediaSource: true }],
  userAgentData: { brands: [{ brand: 'ExampleBrowser', version: '1' }], mobile: true, platform: 'Android', fullVersionList: [{ brand: 'ExampleBrowser', version: '1.0.0' }] },
  refreshHz: 60, webgl: { available: true, vendor: 'ExampleVendor', renderer: 'ExampleRenderer', unmaskedRenderer: 'ExampleGPU' } };

it('summarises a Chromium head unit in one line', () => {
  const [report] = mockReceivers(0);
  expect(receiverSummary(report.info, en)).toBe('Chromium 120 · Adreno 640 · H.264 hardware · H.265 software · 59.9 Hz');
  expect(receiverSummary(report.info, zh)).toBe('Chromium 120 · Adreno 640 · H.264 硬件 · H.265 软件 · 59.9 Hz');
  expect(receiverSummary(vector, en)).toBe('ExampleBrowser 1 · ExampleGPU · H.264 hardware · 60 Hz');
});
it('treats powerEfficient as hardware and keeps the best answer per codec family', () => {
  const entry = (codec: string, efficient: boolean, supported = true) => ({ codec, preferHardware: false, preferSoftware: supported, fps30: { supported, powerEfficient: efficient }, fps60: { supported, powerEfficient: false } });
  const info = { videoDecode: [entry('avc1.42E01F', false), entry('avc1.640028', true), entry('hvc1.1.6.L120.B0', false), entry('av01.0.08M.08', false, false), entry('unknown', true)] };
  expect(codecSupport(info)).toEqual([{ family: 'H.264', support: 'hardware' }, { family: 'H.265', support: 'software' }, { family: 'AV1', support: 'none' }]);
  expect(receiverSummary({ videoDecode: [entry('avc1.640028', true), entry('hev1.1.6.L120.B0', true)] }, en)).toBe('H.264/H.265 hardware');
  expect(codecSupport(null)).toEqual([]);
  expect(codecSupport({ videoDecode: 'bad' })).toEqual([]);
});
it('labels browsers and GPUs defensively', () => {
  expect(browserLabel({ userAgent: 'Mozilla/5.0 Chrome/119.0.1 Safari/537.36' })).toBe('Chromium 119');
  expect(browserLabel({ userAgent: 'Mozilla/5.0 (X11; Linux) Gecko/20100101 Firefox/128.0' })).toBe('Firefox 128');
  expect(browserLabel({ userAgent: 'Mozilla/5.0 (iPad) Version/17.4 Mobile/15E148 Safari/604.1' })).toBe('Safari 17');
  expect(browserLabel({})).toBeNull();
  expect(gpuLabel({ webgl: { unmaskedRenderer: 'ANGLE (Intel, Intel(R) UHD Graphics 620 (0x00005917) Direct3D11 vs_5_0 ps_5_0, D3D11)' } })).toBe('Intel UHD Graphics 620');
  expect(gpuLabel({ webgl: { renderer: 'Mali-G52 MC2' } })).toBe('Mali-G52 MC2');
  expect(gpuLabel({ webgpu: { description: 'PowerVR Rogue GE8320' } })).toBe('PowerVR Rogue GE8320');
  expect(gpuLabel({ webgl: { unmaskedRenderer: 42 } })).toBeNull();
});
it('shows live figures from the latest sample and totals over the window', () => {
  const [report] = mockReceivers(0);
  expect(liveLine(report, en)).toBe('59.8 fps · decode 4/9 ms · 0 dropped · 4 Mbit/s · lag 7 ms');
  expect(windowLine(report, en)).toBe('Last 10 s · 2 dropped · worst decode 23 ms · 1 long tasks');
  expect(liveStats({ ...report, idleSeconds: 8 }).stale).toBe(true);
  const empty: ReceiverReport = { client: '9', idleSeconds: 0, info: null, stats: [] };
  expect(liveLine(empty, en)).toBe('');
  expect(windowLine(empty, en)).toBe('');
});
it('says which codec is used and why in one user line', () => {
  const status = fixture();
  status.video = { codec: 'H265', reason: 'all-main-screen-outputs-hardware-hevc' };
  expect(videoLine(status, en)).toBe('Video: H.265 (all screens decode it in hardware)');
  expect(videoLine(status, zh)).toBe('视频：H.265（所有屏幕都能硬件解码）');
  status.video = { codec: 'H264', reason: 'all-main-screen-outputs-hardware-hevc; codec-change-pending' };
  expect(videoLine(status, en)).toBe('Video: H.264 (all screens decode it in hardware) · changes when CarPlay reconnects');
  status.video = { codec: 'H264', reason: 'main-screen-output-prefers-h264' };
  expect(videoLine(status, en)).toBe('Video: H.264 (a car screen asks for H.264)');
  expect(videoLine(status, zh)).toBe('视频：H.264（车机屏幕要求使用 H.264）');
  status.video = { codec: 'H264', reason: 'relay-h264-only' };
  expect(videoLine(status, en)).toBe('Video: H.264 (this relay uses H.264 for now)');
  expect(videoLine(status, zh)).toBe('视频：H.264（这台中继暂时只用 H.264）');
  status.video = { codec: 'H264', reason: 'something-new' };
  expect(videoLine(status, en)).toBe('Video: H.264');
  expect(videoLine({ video: undefined }, en)).toBe('');
});
it('flags screens that cannot decode the codec in use in hardware', () => {
  const status = fixture(); status.receivers = mockReceivers(0);
  status.video = { codec: 'H265', reason: 'remembered-codec', codecString: 'hvc1.1.6.L120.B0', width: 1920, height: 720 };
  expect(videoView(status)).toMatchObject({ codec: 'H.265', reason: 'remembered-codec', size: '1920 × 720', mismatches: [{ screen: 1, client: '1', support: 'software' }] });
  const html = diagnosticsView(status, en);
  expect(html).toContain('Screen 1 decodes H.265 in software only; video may stutter.');
  expect(html).toContain('<code>hvc1.1.6.L120.B0</code> · 1920 × 720');
  expect(html).toContain('<code>remembered-codec</code>');
  status.video = { codec: 'H264', reason: 'hardware-hevc-unavailable' };
  expect(videoView(status)!.mismatches).toEqual([]);
});
it('renders one compact card per browser with a copy button', () => {
  const status = fixture();
  const second: ReceiverReport = { client: '<2>', idleSeconds: 12, info: null, stats: [] };
  status.receivers = [...mockReceivers(0), second];
  const html = diagnosticsView(status, en);
  expect(html.match(/class="receiver-card"/g)).toHaveLength(2);
  expect(html.match(/data-report=/g)).toHaveLength(2);
  expect(html).toContain('Chromium 120 · Adreno 640 · H.264 hardware · H.265 software · 59.9 Hz');
  expect(html).toContain('Waiting for browser details…');
  expect(html).toContain('Not reporting');
  expect(html).toContain('data-client="&lt;2&gt;"');
  expect(html).not.toContain('<2>');
  expect(diagnosticsView({ ...status, receivers: [] }, en)).toContain('No browser is showing CarPlay right now.');
  expect(diagnosticsView({ ...status, receivers: [] }, zh)).toContain('当前没有浏览器在显示 CarPlay。');
});
it('builds a JSON report for one browser', () => {
  const status = fixture(); status.receivers = mockReceivers(0);
  const report = JSON.parse(receiverReport(status, '1', new Date('2026-10-16T08:00:00Z'))!);
  expect(report).toMatchObject({ report: 'openxplay-receiver', generatedAt: '2026-10-16T08:00:00.000Z', relay: { platform: 'linux', state: 'active', output: 'both' }, video: status.video });
  expect(report.receiver.info.webgl.unmaskedRenderer).toContain('Adreno');
  expect(report.receiver.stats).toHaveLength(10);
  expect(JSON.stringify(report)).not.toContain("Riko's iPhone");
  expect(receiverReport(status, 'missing')).toBeNull();
});
it('copies through the Clipboard API and falls back to a selection copy', async () => {
  const clipboard = { writeText: vi.fn().mockResolvedValue(undefined) };
  const fallback = vi.fn().mockReturnValue(true);
  expect(await copyText('{}', clipboard, true, fallback)).toBe(true);
  expect(clipboard.writeText).toHaveBeenCalledWith('{}');
  expect(fallback).not.toHaveBeenCalled();
  expect(await copyText('{}', clipboard, false, fallback)).toBe(true);
  expect(fallback).toHaveBeenCalledOnce();
  expect(await copyText('{}', { writeText: vi.fn().mockRejectedValue(new Error('denied')) }, true, fallback)).toBe(true);
  expect(fallback).toHaveBeenCalledTimes(2);
  expect(await copyText('{}', undefined, true, () => { throw new Error('blocked'); })).toBe(false);
  expect(await copyText('{}', undefined, true, () => false)).toBe(false);
});
it('shows the user video line on Home only while CarPlay is active', () => {
  const status = fixture(); status.video = { codec: 'H265', reason: 'all-main-screen-outputs-hardware-hevc' };
  expect(devicesView(status, false, en)).toContain('Video: H.265 (all screens decode it in hardware)');
  expect(devicesView(status, false, en)).not.toContain('<button');
  expect(devicesView({ ...status, state: 'waiting' }, false, en)).not.toContain('Video:');
});
it('tells the user only when the Bluetooth guard recovers or gave up', () => {
  const status = fixture();
  expect(statusView(status, en)).not.toContain('Bluetooth');
  status.bluetoothGuard = { state: 'recovering', attempts: 1, lastRecovery: null, lastError: 'hci0 has no address', address: null };
  expect(statusView(status, en)).toContain('<p class="notice" role="status">Bluetooth is restarting…</p>');
  expect(statusView(status, zh)).toContain('蓝牙正在重启…');
  expect(homeView(status).tone).toBe('attention');
  status.bluetoothGuard = { ...status.bluetoothGuard, state: 'failed', attempts: 3 };
  expect(statusView(status, en)).toContain('<p class="error" role="alert">Bluetooth did not start; unplug the relay and plug it back in.</p>');
  expect(statusView(status, zh)).toContain('蓝牙未能启动，请拔下中继后重新插上。');
  delete status.bluetoothGuard;
  expect(statusView(status, en)).not.toContain('Bluetooth');
});
it('always lists the Bluetooth guard in the developer section', () => {
  const status = fixture();
  status.bluetoothGuard = { state: 'ok', attempts: 2, lastRecovery: '2026-10-16T08:00:00Z', lastError: '<reset failed>', address: '00:1A:7D:DA:71:13' };
  const html = diagnosticsView(status, en);
  expect(html).toContain('<h3>Bluetooth</h3>');
  expect(html).toContain('<dt>State</dt><dd>OK</dd><dt>Recovery attempts</dt><dd>2</dd>');
  expect(html).toContain('&lt;reset failed&gt;');
  expect(diagnosticsView({ ...status, bluetoothGuard: { ...status.bluetoothGuard, lastError: null } }, zh)).toContain('<dt>恢复次数</dt><dd>2</dd>');
  expect(diagnosticsView({ ...status, bluetoothGuard: undefined }, en)).not.toContain('<h3>Bluetooth</h3>');
});

it('shows the host model, selected adapter and unmanaged SC policy in both languages', () => {
  const status = fixture();
  status.hostProfile = { model: 'Phicomm N1', profilePath: '/etc/openxplay/host.json', profileLoaded: true, profileError: null,
    bluetoothAdapter: { name: 'hci2', address: 'AA:BB:CC:DD:EE:FF' }, wifiInterface: 'wlan1', wifiPresent: false, secureConnections: 'untouched' };
  for (const t of [en, zh]) {
    const html = diagnosticsView(status, t);
    expect(html).toContain('Phicomm N1');
    expect(html).toContain('hci2 · AA:BB:CC:DD:EE:FF');
    expect(html).toContain(t('host.unmanaged'));
    expect(addressesView(status, false, t)).toContain(t('host.wifiMissing', { interface: 'wlan1' }));
    expect(addressesView(status, false, t)).toContain('role="alert"');
  }
  status.hostProfile.wifiPresent = true;
  expect(addressesView(status, false, en)).not.toContain('role="alert"');
  status.hostProfile.model = '<script>';
  expect(diagnosticsView(status, en)).toContain('&lt;script&gt;');
});

it('renders status without a host profile as before on either relay', () => {
  for (const platform of ['android', 'linux'] as const) {
    const status = fixture();
    status.platform = platform;
    status.host = `openxplay-${platform}.local`;
    expect(status.hostProfile).toBeUndefined();
    for (const t of [en, zh]) {
      const diagnostics = diagnosticsView(status, t);
      expect(diagnostics).toContain(`<summary>${t('diag.video')}</summary>`);
      for (const key of ['host.model', 'host.bluetooth', 'host.sc']) {
        expect(diagnostics).not.toContain(`<dt>${t(key)}</dt>`);
      }
      const addresses = addressesView(status, false, t);
      expect(addresses).toContain(status.host);
      expect(addresses).not.toContain('role="alert"');
    }
  }
});

it('shows host AP restrictions and Bluetooth capability unknowns only when supplied', () => {
  const status = fixture();
  expect(diagnosticsView(status, en)).not.toContain('This host');
  status.hostCapabilities = {
    wifi: { interface: 'wlan0', phy: 'phy0', present: true, capabilities: { modes: ['managed', 'AP'], ap: true, station: true,
      combinations: [{ apStation: true, channels: 1 }], channels: [
        { frequency: 5180, channel: 36, disabled: false, noIr: false, radar: false },
        { frequency: 5200, channel: 40, disabled: false, noIr: true, radar: false },
        { frequency: 5260, channel: 52, disabled: false, noIr: false, radar: true },
        { frequency: 5220, channel: 44, disabled: true, noIr: false, radar: false },
      ] } },
    carlink: { interface: '<wlan1>', phy: null, present: false, capabilities: null },
    regulatory: { global: '00', phy0: 'US' }, adapters: [], selectedAdapter: 'hci0',
    bluetooth: { brEdr: true, le: true, secureConnections: false }, rfcomm: null,
    tools: { nmcli: { present: true, version: '1.30.0' }, dnsmasq: { present: false, version: null } },
  };
  const html = diagnosticsView(status, en);
  expect(html).toContain('This host');
  expect(html).toContain('AP+station: 1 channel(s)');
  expect(html).toContain('5 GHz AP channels</dt><dd>36</dd>');
  expect(html).toContain('phy0: US');
  expect(html).toContain('BR/EDR: Yes · LE: Yes · Secure Connections: No');
  expect(html).toContain('RFCOMM</dt><dd>Unknown');
  expect(html).toContain('Missing tools</dt><dd>dnsmasq');
  expect(html).toContain('&lt;wlan1&gt;');
  expect(html).not.toContain('<wlan1>');
  expect(diagnosticsView(status, zh)).toContain('本机能力');
  expect(diagnosticsView(status, zh)).toContain('监管国家代码');
});

it('formats Linux interface observation times relative to the current snapshot', () => {
  expect(relativeTimestamp(1000, en, 1059)).toBe('59 seconds ago');
  expect(relativeTimestamp(1061, en, 1060)).toBe('0 seconds ago');
  expect(relativeTimestamp(1000, en, 1000 + 150)).toBe('2 min ago');
  expect(relativeTimestamp(1000, en, 1000 + 7300)).toBe('2 h ago');
  expect(relativeTimestamp(1000, (key, args) => translate('zh-CN', key, args), 1059)).toBe('59 秒前');
  expect(relativeTimestamp(1000, (key, args) => translate('zh-CN', key, args), 1000 + 150)).toBe('2 分钟前');
});
