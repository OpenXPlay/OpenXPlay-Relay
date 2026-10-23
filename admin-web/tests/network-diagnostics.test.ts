// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-22

import { expect, it } from 'vitest';
import { networkView, networkCheckMessage, NETWORK_CHECK_CODES } from '../src/network-diagnostics';
import { diagnosticsView } from '../src/diagnostics';
import { translate } from '../src/i18n';
import type { Status, PlannedRole, PlannedNetwork } from '../src/api';
const role: PlannedRole = { interface: { ref: '02:00:00:00:00:01', name: 'wlan0', mac: '02:00:00:00:00:01' }, missing: false, source: 'auto' };
const network: PlannedNetwork = {
  probedAt: 1, applied: false, country: { configured: 'CN', global: 'CN', phys: [{ phy: 'phy0', country: '99', selfManaged: true }] },
  interfaces: [{ name: 'wlan0', mac: '02:00:00:00:00:01', currentMac: '02:00:00:00:00:01', macSource: 'permanent', driver: 'synthetic', bus: 'usb', product: '<script>test</script>', kind: 'wifi', phy: 'phy0', operstate: 'up', ipv4: ['192.0.2.1'],
    wifi: { ap: true, station: true, p2pClient: false, p2pGo: false, concurrency: 'none', modes: ['managed', 'AP'], channels: [], combinations: [], bands: [{ band: '5', stationChannels: [36, 149], apChannels: [149] }] },
    activity: { kind: 'other-ap', profile: 'Foreign <AP>', ssid: '<svg onload=alert(1)>', mode: 'AP', channel: 149, width: '40 MHz' } }],
  plan: { source: 'auto', output: 'both', iphone: { ...role, source: 'legacy', link: 'hotspot', band: '5', channel: 149 }, carBrowser: { ...role, network: 'iphone' }, carlink: { ...role, followOver: true }, adminHotspot: { ...role, enabled: true, ssid: 'Test Admin', band: '5', channel: 0, timeSharedWith: 'carlink', passwordSet: true } },
  checks: [{ level: 'info', code: 'time-shared', roles: ['adminHotspot', 'carlink'], interfaces: ['wlan0'], params: {} }],
};
const fixture = (platform: 'linux' | 'android') => ({ platform, network: { mode: 'device-hotspot', wifi: null, ...network }, receivers: [], video: null } as unknown as Status);
const en = (key: string, params?: Record<string, string | number>) => translate('en', key, params);
const zh = (key: string, params?: Record<string, string | number>) => translate('zh-CN', key, params);
it('shows the Linux inventory, effective country without editing controls', () => {
  const view = networkView(fixture('linux'), en);
  for (const text of ['Interfaces', 'wlan0', 'usb', '192.0.2.1', '5 GHz', 'P2P-client', 'permanent', 'Foreign &lt;AP&gt;', 'CN', 'phy0: 99']) expect(view).toContain(text);
  expect(view).not.toMatch(/<(input|button|select|form)\b/);
  expect(view).not.toContain('<script>'); expect(view).not.toContain('<svg');
  expect(networkView(fixture('linux'), zh)).toContain('网络接口');
  expect(diagnosticsView(fixture('linux'), en)).not.toContain('network-table');
  expect(diagnosticsView(fixture('linux'), en)).toContain('&quot;interfaces&quot;');
});
it('keeps Android diagnostics markup identical even if Linux network data is attached', () => {
  const status = fixture('android');
  const baseline = { ...status, network: { mode: status.network.mode, wifi: status.network.wifi } };
  expect(networkView(status, en)).toBe('');
  expect(diagnosticsView(status, en)).toBe(diagnosticsView(baseline, en));
  expect(diagnosticsView(status, zh)).toBe(diagnosticsView(baseline, zh));
  expect(networkView({ ...status, platform: 'linux', network: baseline.network }, en)).toBe('');
});
it('translates every check and its parameters in English and Chinese', () => {
  for (const code of NETWORK_CHECK_CODES) for (const t of [en, zh]) {
    const message = networkCheckMessage({ level: 'warning', code, roles: ['iphone', 'adminHotspot'], interfaces: ['wlan0'], params: { ref: 'wlan9', band: '5', channels: [36, 40], phy: 'phy0', mhz: 40, connection: 'Foreign AP' } }, t);
    expect(message).not.toContain('roles.check.'); expect(message).not.toMatch(/\{\w+\}/);
  }
  expect(networkCheckMessage({ level: 'warning', code: 'carlink-band-gap', roles: ['carlink'], interfaces: ['wlan1'], params: { channels: [36, 40, 44, 48] } }, en)).toContain('36, 40, 44, 48');
});
it('renders unknown capabilities, pending probes and missing references', () => {
  const status = fixture('linux');
  status.network = { ...status.network, probedAt: null, interfaces: [{ ...network.interfaces[0], wifi: null }], plan: { ...network.plan, carlink: { interface: { ref: '02:00:00:00:00:02', name: null, mac: null }, missing: true, source: 'custom', followOver: false } }, checks: [] };
  const view = networkView(status, en);
  expect(view).toContain('Pending'); expect(view).toContain('Unknown'); expect(view).not.toContain('Planned connections');
});
