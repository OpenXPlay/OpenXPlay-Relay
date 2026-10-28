// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-27

import { describe, expect, it, afterEach, vi } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { mockSettings, previewStatus } from '../dev-server';
import { STAGE_STATES, mockPathInput } from '../dev-fixtures';
import { settingsPlacement, fieldEnabled, fieldRoute, normalizeRoute, parentRoute, pageFields } from '../src/settings-tree';
import { stepCommand, stageCommand } from '../src/path/actions';
import { actions, type StepAction } from '../src/path/model';
import { presentPath } from '../src/path/present-path';
import { presentStage } from '../src/path/present-stage';
import { translate } from '../src/i18n';
import { parseConfig, privacyMarkup } from '../src/feature-pages';
import { api } from '../src/api';
import { validPictureSize, normalizedSizes } from '../src/picture-sizes';
import type { Status } from '../src/api';
const root = new URL('../', import.meta.url);
afterEach(() => vi.unstubAllGlobals());
describe('settings placement', () => {
  it.each([false, true])('has exactly one reachable destination for every field (Linux %s)', linux => {
    const settings = mockSettings(linux), placement = settingsPlacement(settings);
    expect(placement).toHaveLength(settings.schema.flatMap(s => s.fields).length);
    for (const { field, route } of placement) { expect(normalizeRoute('#' + route)).toBe(route); expect(pageFields(settings, route)).toContain(field); }
    expect(new Set(placement.map(p => p.field.key)).size).toBe(placement.length);
  });
  it('covers both real backend schemas without losing hidden network-role fields', () => {
    const android = readFileSync(new URL('../android/app/src/main/java/org/openxplay/direlay/admin/AdminSettingsMapping.kt', root), 'utf8').slice(0).split('fun output(')[0];
    const rust = readFileSync(new URL('../rust/direlay-linux/src/relay_admin.rs', root), 'utf8');
    const linux = rust.slice(rust.indexOf('fn picture_section()'), rust.indexOf('pub fn update_settings('));
    const androidKeys = [...android.matchAll(/(?:AdminField|choice)\("(\w+)"/g)].map(m => m[1]);
    const linuxKeys = [...linux.matchAll(/\("(\w+)",\s*"(?:choice|object|secret|text|networks|number|phone|bool|plugins)"\)/g)].map(m => m[1]);
    linuxKeys.push('resolution', 'screenWidth', 'videoCodec', ...[...linux.matchAll(/field\("(\w+)"/g)].map(m => m[1]));
    expect(androidKeys.length).toBeGreaterThan(20); expect(linuxKeys.length).toBeGreaterThan(25);
    for (const keys of [androidKeys, linuxKeys]) for (const key of keys) expect(fieldRoute(key), key).not.toBe('settings/developer/other');
    for (const key of ['networkRoles', 'adminHotspotPassword', 'wifiInterface', 'carlinkInterface']) expect(fieldRoute(key)).toBe('settings/connection/network/interfaces');
  });
  it('puts unknown keys in Developer › Other, with no developer-mode gate', () => {
    expect(fieldRoute('aFutureBackendField')).toBe('settings/developer/other');
    expect(parentRoute(fieldRoute('aFutureBackendField'))).toBe('settings/developer');
    const source = readFileSync(new URL('src/main.ts', root), 'utf8');
    expect(source).not.toContain('id="developer-options"'); expect(source).not.toContain('more-menu'); expect(source).not.toContain('view-tabs');
  });
  it('supports direct hash navigation and parent/back destinations', () => {
    for (const route of ['settings/output', 'settings/access/admin', 'settings/connection/network/interfaces', 'settings/developer/other']) expect(normalizeRoute('#' + route)).toBe(route);
    expect(parentRoute('settings/connection/network/interfaces')).toBe('settings/connection/network'); expect(parentRoute('settings')).toBe('home'); expect(normalizeRoute('#invalid')).toBe('settings');
  });
});
describe('stage action mapping', () => {
  const unavailable: StepAction[] = ['IMPORT_IDENTITY', 'GRANT_PERMISSIONS', 'KEEP_ALIVE_SETTINGS', 'TURN_ON_BLUETOOTH', 'OPEN_WIFI', 'CHOOSE_RENDERER', 'OPEN_CAST'];
  it.each(['android', 'linux'] as const)('maps every action, including unsupported browser actions (%s)', platform => {
    for (const action of actions) expect(stepCommand(action, platform) === null).toBe(unavailable.includes(action) || platform === 'android' && action === 'OPEN_HOTSPOT');
    for (const [action, endpoint] of [['START_NOW', 'actions/start'], ['RETRY', 'actions/restart'], ['PAIR_IPHONE', 'actions/pair-iphone'], ['RECONNECT_CARLINK', 'carlink/connect']] as const) expect(stepCommand(action, platform)).toEqual({ kind: 'api', endpoint });
    expect(stageCommand({ type: 'Connect' }, platform, true)).toBeNull(); expect(stageCommand({ type: 'Stop' }, platform, false)).toBeNull();
    expect(stageCommand({ type: 'Connect' }, platform, false)).toEqual({ kind: 'api', endpoint: 'actions/start' }); expect(stageCommand({ type: 'Stop' }, platform, true)).toEqual({ kind: 'api', endpoint: 'actions/stop' });
  });
});
it('keeps text whole and motion finite in every stylesheet and template', () => {
  const css = readFileSync(new URL('src/style.css', root), 'utf8').replace(/\.sr-only\s*\{[^}]*\}/g, '');
  const source = readdirSync(new URL('src', root)).filter(f => /\.(ts|css)$/.test(f)).map(f => readFileSync(new URL('src/' + f, root), 'utf8')).join('\n');
  expect(css).not.toMatch(/text-overflow\s*:\s*ellipsis|(?:-webkit-)?line-clamp|white-space\s*:\s*nowrap|animation[^;}]*infinite/i);
  expect(source).not.toMatch(/(?:text-overflow:\s*ellipsis|-webkit-line-clamp|text-overflow\s*=\s*["']ellipsis)/i);
  expect(css).toContain('prefers-reduced-motion'); expect(css).toContain('env(safe-area-inset-bottom)'); expect(css).toContain('touch-action: none'); expect(css).toContain('min-height: 48px');
  expect(readFileSync(new URL('index.html', root), 'utf8')).toContain('viewport-fit=cover');
});
it.each(['en', 'zh-CN'] as const)('translates every new route, path string and privacy paragraph (%s)', lang => {
  const t = (key: string) => translate(lang, key);
  const sources = ['stage.ts', 'settings-tree.ts', 'feature-pages.ts', 'picture-sizes.ts', 'path/texts.ts'].map(f => readFileSync(new URL('src/' + f, root), 'utf8')).join('\n');
  const keys = [...sources.matchAll(/["']((?:menu\.|stage[_.]|files\.|picture\.|path_|error_explain_|logs_)[\w./-]+)["']/g)].map(m => m[1]).filter(k => !k.endsWith('.'));
  for (const key of new Set(keys)) expect(t(key), key).not.toBe(key);
  const notice = privacyMarkup(t); expect(notice).toContain('Cloudflare'); expect(notice).toContain('AWS'); expect(notice).toContain('48'); expect(notice).toContain('72'); expect(notice).toContain('support@openxplay.org');
});
it('validates config size in UTF-8 and requires an envelope object', () => {
  expect(parseConfig('{"format":"openxplay-user-config","settings":{}}')).toMatchObject({ settings: {} });
  for (const text of ['[]', 'null', 'false', '{']) expect(() => parseConfig(text)).toThrow();
  expect(() => parseConfig(JSON.stringify({ note: '中'.repeat(22000) }))).toThrow('files.tooLarge');
});
it('matches Android custom picture sizes, presets, uniqueness and five-size cap', () => {
  expect(validPictureSize({ width: 1920, height: 720 })).toBe(true); expect(validPictureSize({ width: 720, height: 1920 })).toBe(true);
  expect(validPictureSize({ width: 1921, height: 720 })).toBe(false); expect(validPictureSize({ width: 600, height: 320 })).toBe(false);
  const custom = Array.from({ length: 8 }, (_, i) => ({ width: 1300 + i * 2, height: 720 }));
  expect(normalizedSizes([{ width: 1280, height: 720 }, custom[0], ...custom])).toEqual(custom.slice(0, 5));
});
it('uses exact new APIs, never retries an upload or configuration import', async () => {
  const fetch = vi.fn().mockResolvedValue(new Response('{}')); vi.stubGlobal('fetch', fetch);
  await api.logExport('10m'); await api.logUpload('id'); await api.logReceipt(); await api.configImport({ format: 'openxplay-user-config' }, true); await api.configImport({ format: 'openxplay-privacy-config' }); await api.updateCheck('beta');
  expect(fetch.mock.calls.map(([url]) => url)).toEqual(['/api/v0/logs/export', '/api/v0/logs/upload', '/api/v0/logs/receipt', '/api/v0/config/import?dryRun=1', '/api/v0/config/import', '/api/v0/update?channel=beta&fresh=1']);
  fetch.mockClear().mockRejectedValue(new TypeError('lost response')); await expect(api.logUpload('id')).rejects.toThrow(); expect(fetch).toHaveBeenCalledOnce();
});
it('handles an offline update response even when the transport returned 200', async () => {
  vi.stubGlobal('fetch', vi.fn().mockResolvedValue(new Response('{"error":"offline"}')));
  await expect(api.updateCheck('stable')).rejects.toMatchObject({ key: 'offline' });
});

it('disables parked developer and experimental controls without changing saved choices', () => {
  const values = { developerActive: false, experimentalActive: false, carlinkTrace: 'on', experimentalSendBluetoothAddress: true, experimentalBluetoothAddress: '02:12:34:56:78:9a' };
  for (const key of ['carlinkTrace', 'carlinkHevc', 'carlinkPictureSize', 'carlinkModel', 'receiverPort', 'rfcommChannel', 'devicePreset', 'carlinkLink', 'carlinkAddress', 'carlinkHeadUnit', 'carlinkLocal', 'experimentalScreenSessionIsolation', 'experimentalScreenReconnectReset', 'experimentalScreenDeliverySerialization', 'experimentalRemotePlaybackProof', 'experimentalStartupStatus', 'experimentalStartupDiagnostics', 'experimentalSendBluetoothAddress', 'experimentalBluetoothAddress']) expect(fieldEnabled(key, values), key).toBe(false);
  for (const key of ['developerActive', 'experimentalActive', 'hotspotSecurity', 'output']) expect(fieldEnabled(key, values), key).toBe(true);
  values.developerActive = true; values.experimentalActive = true;
  expect(fieldEnabled('carlinkTrace', values)).toBe(true);
  expect(fieldEnabled('experimentalBluetoothAddress', values)).toBe(true);
  expect(values.carlinkTrace).toBe('on');
  expect(values.experimentalSendBluetoothAddress).toBe(true);
});
