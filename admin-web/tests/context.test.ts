// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-16

import { expect, it } from 'vitest';
import { homeView, addressViews, settingsViews, developerPreference, homeButtonCount } from '../src/logic';
import { previewStatus, mockSettings } from '../dev-server';
import { carlinkConnectView, addressesView, statusView, devicesView, nearbyNetworksView } from '../src/views';
import { translate } from '../src/i18n';
import type { Status } from '../src/api';
const fixture = (state: string) => previewStatus({ host: 'openxplay-linux.local', receiverUrls: [] } as unknown as Status, state);
it('chooses only the first applicable action', () => {
  expect(homeView(fixture('fresh')).action).toBe('wifi');
  expect(homeView(fixture('no-iphone')).action).toBe('pair');
  expect(homeView(fixture('carlink-unpaired')).action).toBe('car');
  expect(homeView(fixture('working')).action).toBe(null);
  const state = fixture('no-iphone'); state.carlink!.trusted = false;
  expect(homeView(state).action).toBe('pair');
  state.pairingAvailable = false;
  expect(homeView(state).action).toBe(null);
});
it('counts down to zero and promotes browser addresses without another button', () => {
  const state = fixture('working'); state.pairingDeadline = 120; state.screenConnected = false;
  expect(homeView(state, 115).seconds).toBe(5);
  expect(homeView(state, 121).seconds).toBe(0);
  expect(homeView(state).promoteAddresses).toBe(true);
  state.output = 'carlink'; expect(homeView(state).promoteAddresses).toBe(false);
});
it('filters USB addresses and retains receiver pairing tokens', () => {
  const state = fixture('working'); state.receiverUrls = ['https://192.168.1.78:8443/?code=123'];
  expect(addressViews(state, false)).toHaveLength(1);
  expect(addressViews(state, true)).toHaveLength(2);
  expect(addressViews(state, false)[0].receiver).toContain('?code=123');
  expect(addressViews(state, false)[0].admin).toBe('https://192.168.1.78:6975/');
});
it('keeps technical fields out of user settings', () => {
  const settings = mockSettings(true);
  expect(settingsViews(settings, false).flatMap(s => s.fields).map(f => f.key)).not.toContain('wifiInterface');
  expect(settingsViews(settings, true).find(s => s.id === 'developer')!.fields.map(f => f.key)).toContain('wifiInterface');
});
it('handles denied storage and developer query override', () => {
  const denied = () => { throw new Error('denied'); };
  expect(developerPreference('', denied)).toBe(false);
  expect(developerPreference('?dev=1', denied)).toBe(true);
  expect(developerPreference('', () => '1')).toBe(true);
});
it('understands Linux diagnostic stages and iPhone selection reasons', () => {
  const state = fixture('working'); state.iphone.reason = 'NoIphone';
  expect(homeView(state).action).toBe('pair');
  state.iphone.reason = 'SelectedUnpaired'; expect(homeView(state).action).toBe('pair');
  state.iphone.reason = 'Connected'; state.state = 'WirelessActive';
  expect(homeView(state).phoneConnected).toBe(true);
  expect(homeView(state).hero).toBe('home.screenActive');
});
it('offers two illustrated everyday network choices without removing legacy developer choices', () => {
  const settings = mockSettings();
  const network = (dev: boolean) => settingsViews(settings, dev).flatMap(s => s.fields).find(f => f.key === 'networkMode')!;
  expect(network(false).options).toHaveLength(2);
  expect(network(false).options!.every(o => o.illustration && o.hintKey)).toBe(true);
  expect(network(true).options).toHaveLength(3);
});
it('keeps the live browser pairing code on additional interfaces', () => {
  const state = fixture('working'); state.receiverUrls = ['https://openxplay-linux.local:8443/?code=123'];
  expect(addressViews(state, true).every(a => a.receiver.includes('?code=123'))).toBe(true);
});

it('keeps every preview Home within five visible buttons including More', () => {
  const counts = Object.fromEntries(['fresh', 'no-iphone', 'working', 'carlink-unpaired', 'error'].map(name => [name, homeButtonCount(fixture(name), true)]));
  expect(counts).toEqual({ fresh: 5, 'no-iphone': 5, working: 5, 'carlink-unpaired': 5, error: 5 });
  console.info('Home visible buttons (default password):', counts);
  expect(homeButtonCount(fixture('working'))).toBe(4);
  const state = fixture('working'); state.hostAddresses = [];
  expect(homeButtonCount(state)).toBe(1);
});
it('gives car setup priority over the connected iPhone and password', () => {
  const state = fixture('carlink-unpaired');
  expect(homeView(state, undefined, true).hero).toBe('home.car');
  expect(homeView(state, undefined, true).action).toBe('car');
  state.carlink!.trusted = undefined;
  expect(homeView(state).action).toBe('car');
  expect(homeView(fixture('working'), undefined, true).action).toBe('password');
  expect(homeView(fixture('fresh'), undefined, true).action).toBe('wifi');
  expect(homeView(fixture('no-iphone'), undefined, true).action).toBe('pair');
});
it('hides unknown car rows and models each link independently', () => {
  const state = fixture('carlink-unpaired');
  expect(homeView(state).carRows).toEqual([]);
  expect(homeView(state).phoneConnected).toBe(true);
  expect(homeView(state).carConnected).toBe(false);
  state.carlink!.headUnit = { name: 'My car', address: 'aa:bb' };
  expect(homeView(state).carRows).toEqual([{ label: 'carlink.headUnit', value: 'My car' }]);
});
it('labels addresses with the network name or relay hotspot', () => {
  expect(addressViews(fixture('working'), false)[0].label).toBe('Car Wi-Fi');
  expect(addressViews(fixture('fresh'), false)[0].label).toBe('home.hotspot');
});

it('renders three copy controls, independent link styles and no duplicate hero heading', () => {
  const t = (key: string) => translate('en', key);
  for (const name of ['fresh', 'no-iphone', 'working', 'carlink-unpaired', 'error']) {
    const state = fixture(name);
    const address = addressesView(state, true, t);
    expect(address.match(/<button /g)).toHaveLength(3);
    expect(address.match(/title="Copy address"/g)).toHaveLength(3);
    expect(address).not.toContain('usb0');
    expect(address).not.toContain('data-link-select');
    expect(address).toContain('Car screen</strong>');
    expect(address).toContain('Relay settings</span>');
    const chinese = addressesView(state, false, key => translate('zh-CN', key));
    expect(chinese).toContain('车机屏幕</strong>');
    expect(chinese).toContain('中继设置</span>');
    expect(address).toContain(`data-copy="${addressViews(state, false)[0].admin}"`);
    const hero = statusView(state, t, true);
    expect(hero.match(/<h1 /g)).toHaveLength(1);
    expect(hero).not.toContain('eyebrow');
    expect(devicesView(state, true, t)).not.toContain('<button');
  }
  const hero = statusView(fixture('carlink-unpaired'), t);
  expect(hero).toContain('journey-link connected');
  expect(hero).toContain('journey-link waiting');
  expect(hero.match(/Connect your car/g)).toHaveLength(1);
  expect(hero.match(/Your iPhone is connected/g)).toHaveLength(1);
});
it('uses a network selector to keep the button budget constant with several addresses', () => {
  const state = fixture('working');
  state.hostAddresses!.push({ interface: 'eth0', ipv4: '10.0.0.2', developerOnly: false });
  expect(homeButtonCount(state, true)).toBe(5);
  const html = addressesView(state, false, key => translate('en', key));
  expect(html).toContain('id="address-network"');
  expect(html).toContain('data-address="1" hidden');
  expect(html).toContain('https://10.0.0.2:6975/');
});

it('shows device states in collapsed summaries with text and distinct dot symbols', () => {
  const state = fixture('working');
  const summaries = () => [...devicesView(state, false, key => translate('en', key)).matchAll(/<summary>(.*?)<\/summary>/gs)].map(match => match[1]);
  expect(summaries().every(row => row.includes('Connected') && row.includes('✓'))).toBe(true);
  state.iphone.reason = 'Selected'; state.carlink!.state = 'idle';
  expect(summaries().every(row => row.includes('Not connected') && row.includes('−'))).toBe(true);
  state.iphone.reason = 'SelectedUnpaired'; state.carlink!.trusted = false;
  expect(summaries().every(row => row.includes('Not paired') && row.includes('?'))).toBe(true);
  const html = devicesView(state, false, key => translate('zh-CN', key));
  expect(html).toContain('未配对');
  expect(html).toContain('href="#settings"');
  expect(html).toContain('所选 iPhone 已取消配对。');
});

it('places hotspot instructions and masked copyable credentials before car-screen addresses', () => {
  const state = fixture('hotspot'); state.screenConnected = false;
  const html = addressesView(state, false, (key, parameters) => translate('en', key, parameters));
  expect(html.indexOf('Car screen: join hotspot «OpenXPlay»')).toBeLessThan(html.indexOf('receiver-address'));
  expect(html).toContain('data-copy="OpenXPlay"');
  expect(html).toContain('data-copy="openxplay123"');
  expect(html).toContain('data-secret-toggle aria-pressed="false"');
  expect(html).toContain('••••••••');
  expect(html).toContain('https://10.42.0.1:8443');
  expect(html).toContain('aria-live="polite"');
});

it('shows Carlink stale-code guidance and transient network status in both languages', () => {
  const state = fixture('carlink-unpaired');
  state.network.followingCarlink = true;
  state.carlink!.detail = 'carlink.pinStale';
  for (const language of ['en', 'zh-CN'] as const) {
    const t = (key: string) => translate(language, key);
    expect(statusView(state, t)).toContain(t('network.followingCarlink'));
    const card = carlinkConnectView(state, '', false, '', t);
    expect(card).toContain(t('carlink.pinStale'));
    expect(card).toContain(t('carlink.pinHint'));
    expect(card).toContain('aria-describedby="carlink-connect-hint"');
    state.carlink!.detail = '';
    state.carlink!.lastError = 'carlink.pinStale';
    expect(carlinkConnectView(state, '', false, '', t)).toContain(t('carlink.pinStale'));
  }
  state.network.followingCarlink = false;
  expect(statusView(state, key => key)).not.toContain('network.followingCarlink');
});

it('collapses hotspot credentials once the screen connects in both languages', () => {
  const state = fixture('hotspot');
  state.hotspot!.ssid = '<Relay & Wi-Fi>';
  for (const language of ['en', 'zh-CN'] as const) {
    const t = (key: string, parameters?: Record<string, string | number>) => translate(language, key, parameters);
    for (const connected of [false, true, false]) {
      state.screenConnected = connected;
      const html = addressesView(state, false, t);
      expect(html.includes('hotspot-details')).toBe(!connected);
      expect(html.includes('data-secret=')).toBe(!connected);
      expect(html.includes(state.hotspot!.password)).toBe(!connected);
      expect(html).toContain('&lt;Relay &amp; Wi-Fi&gt;');
      expect(html.includes('class="muted">' + (language === 'en' ? 'Car screen joined hotspot' : '车机已连接热点'))).toBe(connected);
    }
    state.network.mode = 'shared-wifi';
    expect(addressesView(state, false, t)).not.toContain('hotspot-details');
    state.network.mode = 'device-hotspot';
  }
});

it('renders nearby signal levels and accessible security icons with escaped names', () => {
  const networks = [0, 1, 25, 26, 50, 51, 75, 76, 100].map((signal, index) => ({ ssid: `<Wi-Fi & ${index}>`, signal, secure: index % 2 === 0, known: false }));
  for (const language of ['en', 'zh-CN'] as const) {
    const t = (key: string, parameters?: Record<string, string | number>) => translate(language, key, parameters);
    const rows = nearbyNetworksView(networks, t).split('</button>').slice(0, -1);
    expect(rows).toHaveLength(networks.length);
    rows.forEach((row, index) => {
      expect(row).toContain(`data-nearby="${index}"`);
      expect(row).toContain(`&lt;Wi-Fi &amp; ${index}&gt;`);
      expect(row.match(/class="filled"/g)?.length ?? 0).toBe(Math.ceil(networks[index].signal / 25));
      expect(row).toContain(`aria-label="${t('wifi.signal', { signal: networks[index].signal })}"`);
      expect(row.includes(`aria-label="${t('wifi.secured')}"`)).toBe(networks[index].secure);
      expect(row.includes('lucide-lock')).toBe(networks[index].secure);
      expect(row).not.toContain(`>${t('wifi.secured')}</span>`);
    });
    expect(nearbyNetworksView([], t)).toBe('');
  }
});
