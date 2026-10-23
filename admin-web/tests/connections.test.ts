// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-23

import { expect, it } from 'vitest';
import { connectionsView, connectionsForm, interfaceLabel, roleDraft, proposalView, ownAddressWarning, canSaveRoles } from '../src/connections';
import { translate } from '../src/i18n';
import { settingsViews } from '../src/logic';
import type { Status, NetworkInterface, PlannedRole, NetworkProposal } from '../src/api';
import type { Settings } from '../src/schema';
const t = (key: string, params?: Record<string, string | number>) => translate('en', key, params);
const zh = (key: string, params?: Record<string, string | number>) => translate('zh-CN', key, params);
const role = (name: string, index: number): PlannedRole => ({ interface: { ref: `02:00:00:00:00:0${index}`, mac: `02:00:00:00:00:0${index}`, name }, missing: false, source: 'auto' });
function fixture(): Status {
  const iface = (name: string, index: number, product: string, bus: string): NetworkInterface => ({ name, mac: `02:00:00:00:00:0${index}`, currentMac: null, macSource: 'permanent', driver: null, bus, product, kind: 'wifi', phy: `phy${index}`, operstate: 'up', ipv4: [`192.0.2.${index}`],
    wifi: { ap: true, station: true, p2pClient: false, p2pGo: false, concurrency: 'none', modes: ['AP', 'managed'], channels: [], combinations: [], bands: [{ band: '2.4', apChannels: [6], stationChannels: [6] }, { band: '5', apChannels: [149], stationChannels: [36, 149] }] },
    activity: { kind: 'idle', profile: null, ssid: null, mode: null, channel: null, width: null } });
  return { platform: 'linux', output: 'both', hostProfile: { bluetoothAdapter: { name: 'hci0', address: '02:00:00:00:00:04' } }, network: { mode: 'device-hotspot', wifi: null, applied: true, probedAt: 1, checks: [], interfaces: [iface('wlan0', 1, 'BCM43455', 'sdio'), iface('wlan1', 2, 'Realtek RTL8822BU', 'usb'), { ...iface('eth0', 3, 'Ethernet', 'platform'), kind: 'ethernet', wifi: null }],
    plan: { source: 'auto', output: 'both', iphone: { ...role('wlan0', 1), link: 'hotspot', band: '5', channel: 149 }, carBrowser: { ...role('wlan0', 1), network: 'iphone' }, carlink: { ...role('wlan1', 2), followOver: false }, adminHotspot: { interface: null, missing: false, source: 'auto', enabled: false, ssid: '', band: '2.4', channel: 0, timeSharedWith: null, passwordSet: false } } } } as Status;
}
it('shows N1 and RTL8822BU Connections with a collapsed inventory', () => {
  const s = fixture(); const html = connectionsView(s, t);
  for (const text of ['Connections', 'Bluetooth hci0', 'relay hotspot on wlan0', '5 GHz', 'channel 149', 'Realtek RTL8822BU (USB) · 2.4/5 GHz', 'no follow-over', 'Automatic', '192.0.2.2']) expect(html).toContain(text);
  expect(html).toMatch(/<details class="interface-inventory">/); expect(html).not.toContain('Admin hotspot</dt>');
  expect(connectionsView(s, zh)).toContain('连接方式');
  s.output = 'web'; expect(connectionsView(s, t)).not.toContain('Carlink</dt>');
  s.network.plan!.iphone.link = 'saved-network'; expect(connectionsView(s, t)).toContain('saved network on wlan0');
  s.network.plan!.carBrowser = { ...role('eth0', 3), network: 'interface' };
  expect(connectionsView(s, t)).toContain('network of eth0'); expect(connectionsView(s, t)).toContain('192.0.2.3');
});
it('shows waiting roles, probing, mode badge and styled checks', () => {
  const s = fixture(); s.network.plan!.source = 'custom'; s.network.plan!.carlink.missing = true;
  s.network.checks = [{ level: 'info', code: 'interface-busy', roles: ['carlink'], interfaces: ['wlan1'], params: { connection: '<Foreign AP>' } }];
  const html = connectionsView(s, t); expect(html).toContain('Waiting for wlan1'); expect(html).toContain('Custom'); expect(html).toContain('network-check-info'); expect(html).toContain('&lt;Foreign AP&gt;');
  s.network.probing = true; expect(connectionsView(s, t)).toContain('Probing interfaces'); expect(connectionsView(s, t)).toMatch(/id="connections-edit" disabled/);
});
it('offers stable MAC selections and Ethernet only for the car browser', () => {
  const s = fixture(); const draft = roleDraft(s); draft.mode = 'custom'; const form = connectionsForm(s, draft, t);
  expect(form).toContain('value="02:00:00:00:00:02" selected'); expect(form).toContain('Realtek RTL8822BU (USB)');
  const iphone = /data-role="iphone">(.*?)<\/select>/.exec(form)![1]; const browser = /data-role="carBrowser">(.*?)<\/select>/.exec(form)![1];
  expect(iphone).not.toContain('eth0'); expect(browser).toContain('eth0'); expect(browser).toContain('iPhone');
  expect(form).toMatch(/id="roles-save"[^>]+disabled/); expect(form).not.toContain('adminHotspot');
  expect(interfaceLabel({ ...s.network.interfaces![1], product: '<test>' })).toContain('<test>');
  s.network.interfaces![1].product = '<test>'; expect(connectionsForm(s, draft, t)).toContain('&lt;test&gt;');
});
it('disables saving for pending previews and errors, but permits warnings', () => {
  const proposal: NetworkProposal = { plan: fixture().network.plan!, checks: [], changes: [] };
  expect(canSaveRoles(null, false)).toBe(false); expect(canSaveRoles(proposal, true)).toBe(false);
  proposal.checks = [{ level: 'error', code: 'no-ap-mode', roles: ['iphone'], interfaces: ['wlan1'], params: {} }];
  expect(canSaveRoles(proposal, false)).toBe(false); expect(proposalView(fixture(), proposal, 'localhost', t)).toContain('does not support AP mode');
  proposal.checks[0].level = 'warning'; expect(canSaveRoles(proposal, false)).toBe(true);
});
it('reviews reconnects and warns when this IPv4 interface changes role', () => {
  const s = fixture(); const plan = structuredClone(s.network.plan!); plan.iphone.link = 'saved-network';
  const proposal: NetworkProposal = { plan, checks: [], changes: ['controller', 'carlink', 'carBrowserAddress'] };
  const html = proposalView(s, proposal, '192.0.2.1', t);
  for (const text of ['The iPhone reconnects.', 'Carlink reconnects.', 'The car page address changes.', 'It may disconnect']) expect(html).toContain(text);
  expect(ownAddressWarning(s, plan, 'openxplay-linux.local')).toBe(false); expect(ownAddressWarning(s, plan, '192.0.2.2')).toBe(false);
  plan.carlink.interface!.name = 'wlan2'; expect(ownAddressWarning(s, plan, '192.0.2.2')).toBe(true);
  expect(proposalView(s, proposal, '192.0.2.1', zh)).not.toMatch(/connections\./);
});
it('keeps Android markup and developer interface fields unchanged', () => {
  const s = fixture(); s.platform = 'android'; expect(connectionsView(s, t)).toBe(''); expect(connectionsView(s, zh)).toBe('');
  const settings: Settings = { values: {}, schema: [{ id: 'developer', titleKey: '', fields: ['wifiInterface', 'carlinkInterface'].map(key => ({ key, type: 'text', labelKey: key })) }] };
  const android = settingsViews(settings, true, 'android');
  expect(android.flatMap(section => section.fields).map(f => f.key)).toEqual(['wifiInterface', 'carlinkInterface']);
  expect(settingsViews(settings, true, 'linux').flatMap(section => section.fields)).toEqual([]);
});
