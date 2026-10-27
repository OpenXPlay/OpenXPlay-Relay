// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-15

import { expect, it } from 'vitest';
import { sessionRoute, wifiGuidance, sortNetworks, wifiJoinCredentials, carlinkView, carlinkDetail, canConnectCarlink, validCarlinkPin } from '../src/logic';
import { translate } from '../src/i18n';
import type { Status } from '../src/api';
it.each([true, false])('routes sessions independently of default password (%s)', defaultPassword => {
  expect(sessionRoute({ state: 'login', defaultPassword })).toBe('login');
  expect(sessionRoute({ state: 'ready', defaultPassword })).toBe('ready');
});
it('selects join guidance with the destination SSID and Linux admin address', () => {
  expect(translate('en', wifiGuidance('join'), { ssid: 'Home' })).toBe('This relay will close its setup hotspot and join Home. Reconnect this phone or computer to Home, then open https://openxplay-linux.local:6975.');
});
it('selects setup hotspot guidance when forgetting Wi-Fi', () => {
  expect(translate('en', wifiGuidance('forget'))).toContain('OpenXPlay-Setup-XXXX');
  expect(translate('zh-CN', wifiGuidance('join'), { ssid: 'Home' })).toContain('Home');
});
it('sorts known networks first, then by descending signal without mutating the scan', () => {
  const networks = [
    { ssid: 'Strong', signal: 99, known: false, secure: true },
    { ssid: 'Saved weak', signal: 10, known: true, secure: true },
    { ssid: 'Weak', signal: 5, known: false, secure: false },
    { ssid: 'Saved strong', signal: 80, known: true, secure: false },
  ];
  expect(sortNetworks(networks).map(network => network.ssid)).toEqual(['Saved strong', 'Saved weak', 'Strong', 'Weak']);
  expect(networks[0].ssid).toBe('Strong'); expect(sortNetworks([])).toEqual([]);
});

it('joins an unscanned SSID with manual credentials, including open networks', () => {
  expect(wifiJoinCredentials(null, true, 'Hidden network', 'password')).toEqual({ ssid: 'Hidden network', password: 'password' });
  expect(wifiJoinCredentials(null, true, 'Open', '')).toEqual({ ssid: 'Open', password: '' });
  expect(wifiJoinCredentials(null, false, 'Typed', 'password')).toBeNull();
  for (const ssid of ['', '   ', '界'.repeat(11), 'bad\0ssid']) expect(wifiJoinCredentials(null, true, ssid, '')).toBeNull();
  expect(wifiJoinCredentials(null, true, 'a'.repeat(32), '')).not.toBeNull();
});
it('uses scanned security and ignores stale typed credentials for open networks', () => {
  expect(wifiJoinCredentials({ ssid: 'Scanned', signal: 50, secure: false, known: false }, false, 'Typed', 'old')).toEqual({ ssid: 'Scanned', password: '' });
});
it('translates manual Wi-Fi entry and the scan limitation in both languages', () => {
  for (const language of ['en', 'zh-CN'] as const) for (const key of ['wifi.other', 'wifi.enterSsid', 'wifi.scanHint']) expect(translate(language, key)).not.toBe(key);
});

it('shows Carlink for carlink and both outputs and prompts only before the PIN deadline', () => {
  const carlink = { state: 'authenticating', detail: '', headUnit: { name: 'Car', address: 'AA:BB:CC:DD:EE:FF' }, network: { ssid: 'DIRECT-Car', interface: 'wlan0' }, pinRequested: true, pinDeadline: 120 };
  for (const output of ['carlink', 'both']) expect(carlinkView({ output, carlink }, 100)).toEqual({ visible: true, seconds: 20, pinRequested: true });
  expect(carlinkView({ output: 'web', carlink }, 100).pinRequested).toBe(false);
  expect(carlinkView({ output: 'web', carlink }, 100).visible).toBe(false);
  expect(carlinkView({ output: 'both', carlink }, 120)).toEqual({ visible: true, seconds: 0, pinRequested: false });
  expect(carlinkView({ output: 'both', carlink: null }, 100).pinRequested).toBe(false);
  expect(carlinkView({ output: 'both', carlink: { ...carlink, pinRequested: false } }, 100).pinRequested).toBe(false);
  expect(carlinkView({ output: 'both', carlink: { ...carlink, pinDeadline: null } }, 100).pinRequested).toBe(false);
});
it('accepts only 4–8 ASCII digits for a Carlink PIN', () => {
  for (const pin of ['0000', '12345678']) expect(validCarlinkPin(pin)).toBe(true);
  for (const pin of ['', '123', '123456789', '12a4', '１２３４', '1234\n']) expect(validCarlinkPin(pin)).toBe(false);
});
it('translates Carlink settings, states and pairing actions in both languages', () => {
  for (const language of ['en', 'zh-CN'] as const) for (const key of ['field.carlinkInterface', 'hint.field.carlinkInterface', 'field.carlinkLink', 'field.carlinkAddress', 'field.carlinkHeadUnit', 'field.carlinkLocal', 'carlink.scanning', 'carlink.joined', 'carlink.authenticating', 'carlink.playing', 'carlink.enterPin', 'carlink.pinHint', 'carlink.forgetHint', 'error.carlinkPin', 'error.carlinkPinNotRequested']) expect(translate(language, key)).not.toBe(key);
});

it.each(['head_unit_band_unsupported', 'head_unit_band_unsupported:5180'])(
  'explains the unsupported 5 GHz group in both languages (%s)', reason => {
    for (const state of ['lost', 'waiting', 'scanning', 'connecting', 'error']) {
      const key = carlinkDetail({ state, detail: 'link_connect_failed', lastLostReason: reason });
      expect(translate('en', key)).toBe("The head unit's Wi-Fi Direct runs on 5 GHz; this device supports only 2.4 GHz.");
      expect(translate('zh-CN', key)).toBe('车机的 Wi-Fi 直连在 5 GHz，本设备只支持 2.4 GHz。');
    }
    expect(carlinkDetail({ state: 'lost', detail: reason })).toBe('carlink.bandUnsupported5ghz');
  },
);
it('keeps other Carlink details and describes unsupported frequencies accurately', () => {
  expect(carlinkDetail(null)).toBe('');
  expect(carlinkDetail({ state: 'error', detail: 'other-reason' })).toBe('other-reason');
  expect(carlinkDetail({ state: 'joined', detail: '', lastLostReason: null })).toBe('');
  for (const freq of [0, 2399, 2501, 6000, 6500]) {
    const key = carlinkDetail({ state: 'lost', detail: `head_unit_band_unsupported:${freq}` });
    for (const language of ['en', 'zh-CN'] as const) expect(translate(language, key)).not.toBe(key);
    expect(key).toBe('carlink.bandUnsupported');
  }
});

 it('requires a pre-armed PIN for unpaired head units and allows trusted reconnects', () => {
  for (const state of ['unpaired', 'scanning', 'connecting', 'playing', 'lost', 'error']) {
    const c = { state, detail: '', trusted: false };
    expect(canConnectCarlink('', c)).toBe(false);
    expect(canConnectCarlink('012345', c)).toBe(true);
    expect(canConnectCarlink('', { ...c, trusted: true })).toBe(true);
    expect(canConnectCarlink('bad', { ...c, trusted: true })).toBe(false);
  }
  expect(canConnectCarlink('', null)).toBe(false);
});
it('provides the pre-pairing guidance and Connect label in both languages', () => {
  expect(translate('en', 'carlink.pinHint')).toBe('The car screen shows a new code every 2 minutes; connect right after entering it.');
  expect(translate('zh-CN', 'carlink.pinHint')).toBe('车机屏幕上的配对码每 2 分钟更换一次，输入后请马上点连接。');
  for (const lang of ['en', 'zh-CN'] as const) for (const key of ['carlink.connect', 'carlink.unpaired', 'carlink.unpairedHint']) expect(translate(lang, key)).not.toBe(key);
});

import { networkPanelView, switchConsequences, savedNetworkTags, addSavesAndConnects, canConnectNetwork } from '../src/logic';
import { mockSettings, previewStatus } from '../dev-server';
const networkStatus = (state = 'working') => previewStatus({ platform: 'linux', host: 'relay.local', receiverUrls: [] } as unknown as Status, state);
it('models connected, connecting and setup-hotspot status with concrete values', () => {
  const settings = mockSettings(true); const status = networkStatus();
  expect(networkPanelView(status, settings)).toMatchObject({ shared: true, statusKey: 'net.connected', target: 'device-hotspot', parameters: { ssid: 'Car Wi-Fi', ip: '192.168.1.78' } });
  status.network.wifi = null;
  expect(networkPanelView(status, settings).statusKey).toBe('net.connecting');
  const setup = networkPanelView(networkStatus('fresh'), settings);
  expect(setup.statusKey).toBe('net.setup'); expect(setup.parameters.ssid).toBe('OpenXPlay');
});
it('uses the platform hotspot and only asks to add before switching when the list is empty', () => {
  const settings = mockSettings(); const status = networkStatus(); status.platform = 'android';
  expect(networkPanelView(status, settings).target).toBe('wifi-direct');
  const hotspot = networkStatus('hotspot');
  expect(networkPanelView(hotspot, settings)).toMatchObject({ statusKey: 'net.on', target: 'shared-wifi', needsNetwork: false, receiver: 'https://10.42.0.1:8443' });
  settings.values.sharedWifiNetworks = [];
  expect(networkPanelView(hotspot, settings).needsNetwork).toBe(true);
  hotspot.hotspot!.active = false; hotspot.hotspot!.ip = null;
  expect(networkPanelView(hotspot, settings)).toMatchObject({ statusKey: 'net.starts', receiver: null });
});
it('includes the actual Wi-Fi, hotspot credentials, first saved SSID and comeback port', () => {
  const status = networkStatus(); const settings = mockSettings(true);
  settings.values.sharedWifiNetworks = [{ ssid: 'Home Wi-Fi', hasPassword: true }];
  status.adminPort = 7000; status.hotspot = { ssid: 'Relay <&>', password: 'real-secret', ip: '10.42.0.1', active: false };
  expect(switchConsequences(status, settings, 'device-hotspot')).toMatchObject({ change: 'net.leaveWifi', parameters: { wifi: 'Car Wi-Fi', ssid: 'Relay <&>', password: 'real-secret', url: 'https://10.42.0.1:7000' } });
  expect(switchConsequences(status, settings, 'shared-wifi').parameters.wifi).toBe('Home Wi-Fi');
  status.network.wifi = null; status.hotspot!.ip = null;
  expect(switchConsequences(status, settings, 'phone-hotspot')).toMatchObject({ change: 'net.startHotspot', parameters: { url: 'https://relay.local:7000' } });
});
it('tags the live current network before nearby and never uses outdated saved current flags', () => {
  const status = networkStatus(); const scan = [{ ssid: 'Car Wi-Fi', signal: 50, secure: false, known: true }];
  expect(savedNetworkTags({ ssid: 'Car Wi-Fi' }, status, scan)).toEqual(['net.current']);
  expect(savedNetworkTags({ ssid: 'Old', current: true }, status, scan)).toEqual([]);
  expect(savedNetworkTags({ ssid: 'Car Wi-Fi' }, networkStatus('hotspot'), scan)).toEqual(['net.nearby']);
  expect(savedNetworkTags({ ssid: 'Home' }, status, [{ ...scan[0], ssid: 'Home' }])).toEqual(['net.nearby']);
});
it('connects on add only on controllable shared Wi-Fi without a connection', () => {
  const working = networkStatus(); expect(addSavesAndConnects(working)).toBe(false);
  const fresh = networkStatus('fresh'); expect(addSavesAndConnects(fresh)).toBe(true);
  fresh.network.wifi = 'Stale'; expect(addSavesAndConnects(fresh)).toBe(true);
  fresh.wifiControl = false; expect(addSavesAndConnects(fresh)).toBe(false);
  expect(addSavesAndConnects(networkStatus('hotspot'))).toBe(false);
});
it('offers Connect now only for other networks in controllable shared mode', () => {
  const status = networkStatus(); const entry = { ssid: 'Home Wi-Fi' };
  expect(canConnectNetwork(entry, status)).toBe(true);
  expect(canConnectNetwork({ ssid: 'Car Wi-Fi' }, status)).toBe(false);
  status.wifiControl = false; expect(canConnectNetwork(entry, status)).toBe(false);
  expect(canConnectNetwork(entry, networkStatus('hotspot'))).toBe(false);
});
it('caps additions at ten while keeping the list available in either mode', () => {
  const settings = mockSettings(true); settings.values.sharedWifiNetworks = Array.from({ length: 10 }, (_, i) => ({ ssid: String(i) }));
  for (const state of ['working', 'hotspot']) expect(networkPanelView(networkStatus(state), settings)).toMatchObject({ full: true, list: settings.values.sharedWifiNetworks });
});

it('keeps the mode-switch car instruction free of plaintext passwords in both languages', () => {
  const status = networkStatus();
  status.hotspot!.password = 'private-hotspot-secret';
  const consequences = switchConsequences(status, mockSettings(true), 'device-hotspot');
  expect(translate('en', consequences.car, consequences.parameters)).toBe('The car screen must join this hotspot (name and password below).');
  expect(translate('zh-CN', consequences.car, consequences.parameters)).toBe('车机需要连接这个热点（名称和密码见下方）。');
});
