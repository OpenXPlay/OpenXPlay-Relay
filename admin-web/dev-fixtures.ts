// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-27

import type { Status } from './src/api';
import { normalizeInput, type PathInput } from './src/path/model';
const previewEpoch = Date.now();
export const STAGE_STATES = ['idle', 'ready', 'pairing', 'carlink-scanning', 'carlink-pin', 'failure', 'car-first-wait', 'live', 'arrival', 'armed', 'identity-missing', 'bluetooth-off', 'wifi-off', 'multiple-iphones', 'unpaired-iphone', 'carlink-confirm', 'carlink-pin-rejected', 'startup-bluetooth', 'startup-auth', 'startup-wifi', 'startup-session', 'display-changed', 'feature-empty', 'feature-error'] as const;
/** The same facts contract is used for Android and Linux previews. */
export function mockPathInput(status: Status, state = '', nowMillis = Date.now(), phaseEpoch = previewEpoch): PathInput {
  const input = normalizeInput({ platform: status.platform, nowMillis, version: status.version,
    setup: { network: status.network.mode === 'shared-wifi' ? 'SHARED_WIFI' : status.network.mode === 'wifi-direct' ? 'WIFI_DIRECT' : 'PHONE_HOTSPOT', outputs: status.output === 'web' ? ['BROWSER'] : status.output === 'carlink' ? ['CARLINK'] : ['BROWSER', 'CARLINK'], summary: '', valid: true },
    relay: { identity: 'READY', identityLabel: 'Installed', bluetoothOn: true, bluetoothName: status.platform === 'linux' ? 'OpenXPlay Linux' : 'OpenXPlay Relay', wifiOn: true, wifiSsid: status.network.wifi, hotspotOn: !!status.hotspot?.active, hotspotSsid: status.hotspot?.ssid ?? 'OpenXPlay-1A2B', hotspotBand: '5 GHz', sharedWifiSsid: status.network.wifi },
    car: { address: status.hostAddresses?.[0]?.ipv4 ?? '10.42.0.1', pages: status.screenConnected ? 1 : 0, screen: status.screenConnected ? '1920×720' : null, savedScreen: '1920×720', pageUrl: status.receiverUrls[0] ?? 'https://10.42.0.1:8443/', web: { phase: 'LISTENING', port: 8443, code: null, trustedName: status.publicName?.trustedName ?? null, sinceMillis: phaseEpoch - 60000 }, carlink: { phase: status.running ? 'PLAYING' : 'SCANNING', sinceMillis: phaseEpoch - 12000, carName: 'G6', code: null, error: null, retrying: false, joinMethod: 'Wi-Fi', trusted: true, pinPending: false, picture: '1920×720 H.265', trail: [] } },
    iphone: { pick: status.iphone.name ? 'PICKED' : 'NONE', name: status.iphone.name, started: status.running, step: status.state === 'active' ? 'VIDEO' : status.running ? 'BLUETOOTH' : 'IDLE', stepSinceMillis: phaseEpoch - 12000 },
    live: { pictureOnCar: status.state === 'active' && !!status.screenConnected, sessionSinceMillis: phaseEpoch - 751000, chips: ['1920×720', '60 fps', 'H.265', '8.2 Mb/s'] },
  });
  if (state === 'idle') { input.relay.identity = 'MISSING'; input.iphone.started = false; input.iphone.step = 'IDLE'; input.live.pictureOnCar = false; }
  if (state === 'ready') { input.iphone.started = false; input.iphone.step = 'IDLE'; input.live.pictureOnCar = false; input.car.carlink.phase = 'PLAYING'; }
  if (state === 'pairing' || state === 'no-iphone' || state === 'fresh') { input.iphone.pick = 'NONE'; input.iphone.name = null; input.iphone.started = false; input.iphone.step = 'IDLE'; input.live.pictureOnCar = false; }
  if (state === 'carlink-scanning' || state === 'carlink-pin') {
    input.setup.outputs = ['CARLINK']; input.car.carlink.phase = state === 'carlink-pin' ? 'PIN' : 'SCANNING'; input.car.carlink.pinPending = state === 'carlink-pin'; input.car.carlink.trusted = false; input.iphone.started = true; input.iphone.step = 'STARTING'; input.live.pictureOnCar = false;
  }
  if (state === 'failure' || state === 'error') { input.iphone.started = true; input.iphone.step = 'JOINING'; input.iphone.failure = { step: 'JOINING', code: 'first_tcp_timeout', text: 'The iPhone did not join the hotspot', atMillis: phaseEpoch - 30000, retrying: false, trail: ['Wi-Fi details sent', 'Waiting for the iPhone TCP connection'] }; input.live.pictureOnCar = false; }
  if (state === 'car-first-wait') { input.car.pages = 0; input.car.screen = null; input.car.savedScreen = null; input.iphone.started = true; input.iphone.step = 'STARTING'; input.iphone.carWaitUntilMillis = phaseEpoch + 12000; input.live.pictureOnCar = false; }
  if (state === 'live' || state === 'working') { input.iphone.started = true; input.iphone.step = 'VIDEO'; input.car.pages = 1; input.car.screen = '1920×720'; input.car.carlink.phase = 'PLAYING'; input.live.pictureOnCar = true; }
  return input;
}
export function mockNetwork(mode: string): Status['network'] {
  const role = { interface: { ref: '02:42:00:00:00:01', name: 'wlan0', mac: '02:42:00:00:00:01' }, missing: false, source: 'auto' as const };
  return { mode, wifi: mode === 'shared-wifi' ? 'Car Wi-Fi' : null, probedAt: Math.floor(Date.now() / 1000), applied: true, country: { global: 'US', configured: 'US', phys: [{ phy: 'phy0', country: 'US', selfManaged: false }] }, checks: [],
    interfaces: [{ name: 'wlan0', mac: '02:42:00:00:00:01', currentMac: '02:42:00:00:00:01', macSource: 'permanent', driver: 'brcmfmac', bus: 'sdio', product: 'Wi-Fi adapter', kind: 'wifi', phy: 'phy0', operstate: 'up', ipv4: ['10.42.0.1'], wifi: { ap: true, station: true, p2pClient: true, p2pGo: true, concurrency: 'ap-station', modes: ['AP', 'managed', 'P2P-client'], channels: [{ channel: 36, frequency: 5180, disabled: false, noIr: false, radar: false }], combinations: [{ modes: ['AP', 'managed'], channels: 1, apStation: true, description: 'AP + station, same channel' }], bands: [{ band: '2.4', stationChannels: [1, 6, 11], apChannels: [1, 6, 11] }, { band: '5', stationChannels: [36, 40, 44, 48], apChannels: [36, 40, 44, 48] }] }, activity: { kind: 'iphone-hotspot', profile: 'OpenXPlay', ssid: 'OpenXPlay-1A2B', mode: 'ap', channel: 36, width: '20 MHz' } }],
    plan: { source: 'auto', output: 'both', iphone: { ...role, link: mode === 'shared-wifi' ? 'saved-network' : 'hotspot', band: '5', channel: 36 }, carBrowser: { ...role, network: 'iphone' }, carlink: { ...role, followOver: true }, adminHotspot: { ...role, enabled: false, ssid: 'OpenXPlay-Admin', band: '2.4', channel: 6, timeSharedWith: 'carlink', passwordSet: false } } };
}
