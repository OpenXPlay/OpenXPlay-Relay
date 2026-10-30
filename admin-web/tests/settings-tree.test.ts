// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-30

import { describe, expect, it } from 'vitest';
import { linuxSettings, linuxStatusFixtures, mockSettings } from '../dev-server';
import { translate } from '../src/i18n';
import { icon } from '../src/icons';
import { icons } from '../src/lucide-icons';
import { accessIcons, experimentalGroups, experimentalMarkup, fieldEnabled, fieldRoute, menuSummary, pageAvailable, pageFields, pages } from '../src/settings-tree';
import type { Field, Settings } from '../src/schema';

const status = linuxStatusFixtures.ready;
const newOptions = ['experimentalIapChannelWhenOffered', 'experimentalReleaseBluetoothOnRequest', 'experimentalAwaitIphoneReconnect', 'experimentalRadioConflictPrompt', 'experimentalStallWatchdog'];
const screenOptions = ['experimentalScreenSessionIsolation', 'experimentalScreenReconnectReset', 'experimentalScreenDeliverySerialization'];
const progressOptions = ['experimentalRemotePlaybackProof', 'experimentalCancelRecoveredRetries', 'experimentalStartupStatus', 'experimentalStartupDiagnostics', 'experimentalNetworkObservationBackoff'];
const addressOptions = ['experimentalSendBluetoothAddress', 'experimentalBluetoothAddress'];
const experimentalFields = (keys: string[]): Field[] => keys.map(key => ({ key, labelKey: `field.${key}`, type: key === 'experimentalBluetoothAddress' ? 'text' : 'bool' }));
const settingsWith = (fields: Field[], values: Settings['values']): Settings => ({ schema: [{ id: 'experimental', titleKey: 'section.experimental', fields }], values });
const renderField = (field: Field) => `<div data-field="${field.key}"></div>`;

describe.each(['en', 'zh-CN'] as const)('quiet menu summaries (%s)', language => {
  const t = (key: string) => translate(language, key);
  const summary = (route: string, values: Settings['values'] = {}, settings = linuxSettings) => menuSummary({ ...settings, values: { ...settings.values, ...values } }, status, `settings/${route}`, language, t);
  it('omits automatic picture settings and the default physical width, even with a measured rate', () => {
    expect(menuSummary(linuxSettings, { ...status, running: true, video: { ...status.video, appliedFps: 60 } }, 'settings/picture', language, t)).toBe('');
    expect(summary('picture', { frameRate: '60' })).toBe('60 fps');
    expect(summary('picture', { resolution: '720p', screenWidth: 250, videoCodec: 'h264', frameRate: '30' })).toBe(['720p', '250 mm', 'H.264', '30 fps'].join(' · '));
  });
  it('supports Android picture field names without inventing missing settings', () => {
    const settings: Settings = { schema: [{ id: 'picture', titleKey: '', fields: [{ key: 'carplayResolution', labelKey: '', type: 'choice', options: [{ value: 'native', labelKey: '', labels: { en: 'Native', 'zh-CN': '原生' } }] }, { key: 'carplayScreenWidth', labelKey: '', type: 'number' }] }], values: {} };
    expect(summary('picture', {}, settings)).toBe('');
    expect(summary('picture', { carplayResolution: 'native', carplayScreenWidth: 300 }, settings)).toBe(`${language === 'en' ? 'Native' : '原生'} · 300 mm`);
  });
  it('omits default startup, access, audio and developer settings', () => {
    for (const route of ['startup', 'access', 'audio', 'developer/carlink']) expect(summary(route), route).toBe('');
    expect(summary('startup', { autoStart: false })).toBe(`${t('menu.whenReady')}: ${t('stage.off')}`);
    expect(summary('startup', { bootStart: false })).toBe(`${t('menu.atBoot')}: ${t('stage.off')}`);
    expect(summary('audio', { carBluetoothAudio: false })).toBe(`${t('stage.carBluetooth')}: ${t('stage.off')}`);
    expect(summary('access', { publicNameEnabled: true })).toBe(`${t('publicName.title')}: ${t('stage.on')}`);
    expect(menuSummary(linuxSettings, { ...status, adminEnabled: false }, 'settings/access', language, t)).toBe(`${t('menu.admin')}: ${t('stage.off')}`);
    expect(summary('developer/video', { displayMode: 'auto' }, mockSettings(false))).toBe('');
  });
  it('does not summarize values absent from the schema or invisible fields', () => {
    const empty: Settings = { schema: [], values: {} };
    for (const route of ['picture', 'startup', 'audio', 'access', 'connection']) expect(summary(route, { resolution: '720p', screenWidth: 300, frameRate: '60', autoStart: false, carBluetoothAudio: false, publicNameEnabled: true, transport: 'usb' }, empty)).toBe('');
    const settings = structuredClone(linuxSettings);
    settings.schema.flatMap(section => section.fields).find(field => field.key === 'frameRate')!.visibleWhen = { key: 'output', equals: ['carlink'] };
    expect(summary('picture', { output: 'web', frameRate: '60' }, settings)).toBe('');
  });
  it('counts only schema-present enabled switches and excludes master and Bluetooth text', () => {
    const fields = experimentalFields(['experimentalActive', newOptions[0], newOptions[1], 'experimentalBluetoothAddress']);
    const values = { experimentalActive: true, [newOptions[0]]: true, [newOptions[1]]: false, [newOptions[2]]: true, experimentalBluetoothAddress: '02:12:34:56:78:90' };
    const settings = settingsWith(fields, values);
    expect(summary('experimental', {}, settings)).toBe(`${t('stage.on')} · 1 ${t('menu.options')}`);
    expect(summary('experimental', { experimentalActive: false }, settings)).toBe(t('stage.off'));
    expect(summary('experimental', { [newOptions[0]]: false }, settings)).toBe(t('stage.on'));
  });
});

describe('Tesla availability', () => {
  it.each([
    ['', 'off', false], ['xpeng', 'off', false], ['tesla', 'off', true],
    ['xpeng', 'teslamirror', true], ['', 'virtual-name', true], ['', 'public-address', true], ['', 'tesla-android', true],
    ['', undefined, false], ['tesla', undefined, true],
  ])('gates Linux brand %s and mode %s', (carBrand, teslaBrowser, available) => {
    const settings = structuredClone(linuxSettings);
    settings.values.carBrand = carBrand!;
    if (teslaBrowser === undefined) delete settings.values.teslaBrowser;
    else settings.values.teslaBrowser = teslaBrowser;
    expect(pageAvailable(settings, status, 'settings/tesla')).toBe(available);
  });
  it('keeps Android visibility based on the schema, including Off for other brands', () => {
    expect(pageAvailable(linuxSettings, { ...status, platform: 'android' }, 'settings/tesla')).toBe(true);
    expect(pageAvailable(mockSettings(false), { ...status, platform: 'android' }, 'settings/tesla')).toBe(false);
    expect(pageAvailable({ schema: [], values: { carBrand: 'tesla' } }, status, 'settings/tesla')).toBe(false);
  });
});

describe('experimental release groups', () => {
  const allKeys = ['experimentalActive', ...newOptions, ...screenOptions, ...progressOptions, ...addressOptions];
  it('renders Linux as one flat 0.1.6 group, with new options first and the master outside', () => {
    const keys = ['experimentalActive', ...newOptions, ...screenOptions, ...progressOptions.filter(key => !['experimentalCancelRecoveredRetries', 'experimentalNetworkObservationBackoff'].includes(key)), ...addressOptions];
    const fields = experimentalFields([...keys].reverse());
    const groups = experimentalGroups(fields, 'linux');
    expect(groups.map(group => group.version)).toEqual(['0.1.6']);
    expect(groups[0].fields.map(field => field.key)).toEqual(keys.slice(1));
    const markup = experimentalMarkup(fields, 'linux', renderField);
    expect(markup).not.toContain('<h2>');
    expect(markup).toContain(`${renderField(fields.find(field => field.key === 'experimentalActive')!)}</section><section`);
    expect(markup.indexOf(newOptions[0])).toBeLessThan(markup.indexOf(screenOptions[0]));
  });
  it('matches Android release order, renders headings and keeps every field exactly once', () => {
    const fields = experimentalFields([...allKeys].reverse());
    const groups = experimentalGroups(fields, 'android');
    expect(groups.map(group => [group.version, group.fields.map(field => field.key)])).toEqual([
      ['0.2.3', newOptions], ['0.2.2', screenOptions], ['0.2.1', progressOptions], ['0.2.0', addressOptions],
    ]);
    const markup = experimentalMarkup(fields, 'android', renderField);
    expect([...markup.matchAll(/<h2>(.*?)<\/h2>/g)].map(match => match[1])).toEqual(['0.2.3', '0.2.2', '0.2.1', '0.2.0']);
    for (const key of allKeys) expect(markup.split(`data-field="${key}"`)).toHaveLength(2);
    expect(markup.indexOf('experimentalActive')).toBeLessThan(markup.indexOf('<h2>'));
  });
  it('skips absent schema keys and empty groups, with no header for one remaining group', () => {
    const fields = experimentalFields(['experimentalActive', 'experimentalStartupDiagnostics']);
    expect(experimentalGroups(fields, 'android').map(group => group.version)).toEqual(['0.2.1']);
    expect(experimentalMarkup(fields, 'android', renderField)).not.toContain('<h2>');
    expect(experimentalMarkup([], 'android', renderField)).toBe('');
    const settings = mockSettings(false);
    expect(experimentalGroups(pageFields(settings, 'settings/experimental'), 'android')).toEqual([]);
  });
  it('routes the new and Android-only switches to Experimental and retains parked values', () => {
    const values = Object.fromEntries(allKeys.map(key => [key, key !== 'experimentalActive']));
    for (const key of allKeys) {
      expect(fieldRoute(key)).toBe('settings/experimental');
      expect(fieldEnabled(key, values)).toBe(key === 'experimentalActive');
    }
    const fields = experimentalFields(allKeys);
    const markup = experimentalMarkup(fields, 'android', field => `<input data-field="${field.key}" ${fieldEnabled(field.key, values) ? '' : 'disabled'}>`);
    expect(markup.match(/disabled/g)).toHaveLength(allKeys.length - 1);
    values.experimentalActive = true;
    for (const key of allKeys) { expect(fieldEnabled(key, values)).toBe(true); expect(values[key]).toBe(true); }
  });
});

it('uses distinct, available icons for the audited menu and access rows', () => {
  const mapping = Object.fromEntries(pages.map(([page, , name]) => [page, name]));
  expect(Object.fromEntries(['output', 'car', 'tesla', 'config-files', 'developer', 'experimental', 'access'].map(page => [page, mapping[page]]))).toEqual({ output: 'cast', car: 'car', tesla: 'globe', 'config-files': 'files', developer: 'code', experimental: 'flask-conical', access: 'key-round' });
  const names = [mapping.access, accessIcons.publicName, accessIcons.admin];
  expect(new Set(names).size).toBe(3);
  for (const name of [...Object.values(mapping), ...names, 'download', 'external-link']) {
    expect(icons[name], name).toContain(`lucide-${name}`);
    expect(icon(name)).toContain('aria-hidden="true" focusable="false"');
  }
  for (const name of ['cast', 'globe', 'files', 'flask-conical', 'key-round', 'shield-check', 'download', 'external-link']) expect(icons[name]).toContain('@license lucide-static v1.57.0 - ISC');
});
