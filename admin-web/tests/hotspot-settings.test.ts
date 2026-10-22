// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-16

import { expect, it } from 'vitest';
import { hotspotForm, validateHotspot, restartRequired, restartView, stationBandHint, devicePresetLabel } from '../src/hotspot-settings';
import { translate } from '../src/i18n';
import type { Status, Wireless } from '../src/api';
import { mockSettings, previewStatus } from '../dev-server';
import { settingsViews } from '../src/logic';
import { hotspotDetails } from '../src/views';
import { diagnosticsView } from '../src/diagnostics';
import { createHash } from 'node:crypto';
const wireless = (): Wireless => ({
  preset: { id: 'phicomm-n1', label: 'Phicomm N1', source: 'detected', detected: 'phicomm-n1' },
  mode: 'concurrent', station24: true, ap24: true, station5: true, ap5: true,
  apStation: true, apStationChannels: 1, p2pClient: true, ap5Channels: [36, 40, 44, 48, 149, 153, 157, 161, 165],
  hotspot: { band: '5', autoBand: '5', channel: 36, configuredBand: 'auto', configuredChannel: 0, reason: null },
});
it('validates UTF-8 name bytes and printable ASCII password boundaries', () => {
  for (const name of ['', 'a'.repeat(33), '中'.repeat(11)]) expect(validateHotspot(name, 'abcdefghij')).toBe('error.hotspotSsid');
  for (const name of ['a', 'a'.repeat(32), '中'.repeat(10), 'a\nb']) expect(validateHotspot(name, ' '.repeat(8))).toBe('');
  for (const password of ['1234567', 'a'.repeat(64), '1234567\n', '1234567é', '1234567\x7f']) expect(validateHotspot('Relay', password)).toBe('error.hotspotPassword');
  expect(validateHotspot('Relay', '~'.repeat(63))).toBe('');
});
it('renders labelled, masked editable fields and a review step in both languages', () => {
  for (const language of ['en', 'zh-CN'] as const) {
    const t = (key: string) => translate(language, key);
    const html = hotspotForm('<Relay & car>', t);
    expect(html).toContain('&lt;Relay &amp; car&gt;');
    expect(html).toContain('for="hotspot-name"');
    expect(html).toContain('for="hotspot-password"');
    expect(html).toContain('type="password"');
    expect(html).toContain('aria-pressed="false"');
    expect(html).toContain(t('ui.reveal'));
    expect(html).toContain(t('hotspot.review'));
    expect(html).toContain('role="alert"');
    for (const key of ['hotspot.change', 'hotspot.confirm', 'hotspot.apply', 'hotspot.warning', 'hotspot.later', 'hotspot.reconnecting', 'error.hotspotSsid', 'error.hotspotPassword', 'wifi.only24']) expect(t(key)).not.toBe(key);
    expect(t('wifi.only24')).toContain('2.4 GHz');
    expect(t('wifi.joinFailed.notFound')).toContain('2.4 GHz');
  }
});
it('keeps restart available from a saved response and after page reload', () => {
  expect(restartRequired([], 'next-start')).toBe(true);
  expect(restartRequired(['notice.nextStart'])).toBe(true);
  expect(restartRequired([], 'now')).toBe(false);
  for (const language of ['en', 'zh-CN'] as const) {
    const t = (key: string) => translate(language, key);
    expect(restartView(false, t)).toBe('');
    expect(restartView(true, t)).toContain(t('ui.restartRequired'));
    expect(restartView(true, t)).toContain(t('ui.restartNow'));
    expect(restartView(true, t)).toContain('id="restart-now"');
  }
});
it('renders the Linux band select with preset defaults and only disables known unsupported 5 GHz', () => {
  for (const language of ['en', 'zh-CN'] as const) {
    const t = (key: string, parameters?: Record<string, string | number>) => translate(language, key, parameters);
    const w = wireless();
    const html = hotspotForm('Relay', t, w);
    expect(html).toContain('for="hotspot-band"');
    expect(html).toContain('<select id="hotspot-band">');
    expect(html).toContain(t('hotspot.automatic', { band: '5 GHz', preset: t('option.devicePreset.phicomm-n1') }));
    expect(html).toContain('value="auto" selected');
    expect(html).not.toContain(' disabled');
    w.hotspot.autoBand = '2.4'; w.ap5 = false; w.station5 = false; w.preset.id = 'msm8916'; w.hotspot.configuredBand = '2.4';
    const limited = hotspotForm('Relay', t, w);
    expect(limited).toContain('value="5" disabled');
    expect(limited).toContain('value="2.4" selected');
    expect(limited).toContain(t('hotspot.no5'));
    expect(limited).toContain(t('hotspot.automatic', { band: '2.4 GHz', preset: t('option.devicePreset.msm8916') }));
    w.ap5 = null;
    expect(hotspotForm('Relay', t, w)).not.toContain(' disabled');
    expect(hotspotForm('Relay', t)).not.toContain(' disabled');
  }
});
it('shows the station band hint only for a Linux radio known to lack 5 GHz', () => {
  for (const language of ['en', 'zh-CN'] as const) {
    const t = (key: string) => translate(language, key);
    for (const platform of ['linux', 'android'] as const) for (const station5 of [true, false, null]) {
      const w = { ...wireless(), station5 };
      expect(stationBandHint({ platform, wireless: w }, t)).toBe(platform === 'linux' && station5 === false ? `<p>${t('wifi.only24')}</p>` : '');
    }
    expect(stationBandHint({ platform: 'linux' }, t)).toBe('');
    expect(stationBandHint({ platform: 'android' }, t)).toBe('');
  }
});
it('uses the resolved automatic band even when a preset or capability shortcut would disagree', () => {
  const t = (key: string, parameters?: Record<string, string | number>) => translate('en', key, parameters);
  const w = wireless();
  w.hotspot.autoBand = '2.4';
  expect(hotspotForm('Relay', t, w)).toContain(t('hotspot.automatic', { band: '2.4 GHz', preset: t('option.devicePreset.phicomm-n1') }));
  w.preset.id = 'msm8916'; w.hotspot.autoBand = '5';
  expect(hotspotForm('Relay', t, w)).toContain(t('hotspot.automatic', { band: '5 GHz', preset: t('option.devicePreset.msm8916') }));
});
it('keeps the preset visible to users and the Android hotspot band in its original developer section', () => {
  for (const developer of [true, false]) {
    const linux = settingsViews(mockSettings(true), developer, 'linux');
    expect(linux.find(s => s.id === 'connection')!.fields.some(f => f.key === 'devicePreset')).toBe(true);
    expect(linux.flatMap(s => s.fields).some(f => f.key === 'hotspotBand')).toBe(false);
    const android = settingsViews(mockSettings(), developer, 'android');
    expect(android).toEqual(settingsViews(mockSettings(), developer));
    expect(android.flatMap(s => s.fields).some(f => f.key === 'hotspotBand')).toBe(developer);
    if (developer) expect(android.find(s => s.id === 'developer')!.fields.some(f => f.key === 'hotspotBand')).toBe(true);
  }
  for (const language of ['en', 'zh-CN'] as const) {
    const t = (key: string, parameters?: Record<string, string | number>) => translate(language, key, parameters);
    const w = wireless(); w.preset.id = 'generic';
    expect(devicePresetLabel('auto', { platform: 'linux', wireless: w }, t)).toBe(t('preset.automatic', { label: t('option.devicePreset.phicomm-n1') }));
    for (const id of ['msm8916', 'phicomm-n1', 'generic']) expect(devicePresetLabel(id, null, t)).toBe(t(`option.devicePreset.${id}`));
  }
});
it('adds Linux hotspot band, working mode and diagnostics, preserving Android hotspot markup', () => {
  const s = previewStatus({ platform: 'linux', host: 'openxplay-linux.local', receiverUrls: [] } as unknown as Status, 'hotspot');
  s.wireless = wireless();
  for (const language of ['en', 'zh-CN'] as const) {
    const t = (key: string, parameters?: Record<string, string | number>) => translate(language, key, parameters);
    const html = hotspotDetails(s, t);
    expect(html).toContain(t('hotspot.radio', { band: '5 GHz', channel: 36 }));
    expect(html).toContain(t('wireless.mode.concurrent'));
    expect(diagnosticsView(s, t)).toContain('apStationChannels');
    s.wireless.mode = 'single-radio';
    expect(hotspotDetails(s, t)).toContain(t('wireless.mode.single-radio'));
    s.wireless.mode = 'unknown';
    expect(hotspotDetails(s, t)).not.toContain(t('wireless.mode.concurrent'));
    s.wireless.mode = 'concurrent';
  }
  const android = { ...s, platform: 'android' as const, wireless: undefined };
  const t = (key: string) => translate('en', key);
  const html = hotspotDetails(android, t);
  // Golden output for the existing Android hotspot details, including controls and placement.
  expect(createHash('sha256').update(html).digest('hex')).toBe('63fa1b95b6b8e507b0b677cecd7b4a7dd3ffc0fcb485742ebdb76ad6e83b0334');
  expect(hotspotDetails({ ...android, wireless: wireless() }, t)).toBe(html);
  expect(diagnosticsView({ ...android, wireless: wireless() }, t)).toBe(diagnosticsView(android, t));
});
