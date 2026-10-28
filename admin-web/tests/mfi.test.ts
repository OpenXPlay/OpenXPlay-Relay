// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-27

import { expect, it } from 'vitest';
import { readFileSync } from 'node:fs';
import { mfiError, mfiReadiness, mfiSubmission } from '../src/mfi';
import { fieldRoute, pageAvailable } from '../src/settings-tree';
import { isVisible, type Settings } from '../src/schema';
import { translate } from '../src/i18n';
import { publicNameConfigured } from '../src/public-name';
import { linuxStatusFixtures } from '../dev-server';

it.each(['remote', 'i2c', 'usb-ch341'])('places the production %s controls and readiness in Identity', source => {
  const settings = JSON.parse(readFileSync(new URL(`./fixtures/linux/settings-mfi-${source}.json`, import.meta.url), 'utf8')) as Settings;
  const fields = settings.schema.flatMap(section => section.fields);
  const target = fields.find(field => field.key === 'mfiTarget')!;
  expect(target.options?.map(option => option.value)).toEqual(['local', 'remote', 'usb-ch341', 'i2c']);
  for (const key of ['mfiTarget', 'mfiI2cPath', 'mfiRemoteServer', 'mfiRemoteToken']) expect(fieldRoute(key)).toBe('settings/identity');
  expect(pageAvailable(settings, linuxStatusFixtures.ready, 'settings/identity')).toBe(true);
  expect(fields.find(field => field.key === 'mfiRemoteToken')!.type).toBe('secret');
  expect(settings.values.mfiRemoteToken).toBeUndefined();
  expect(settings.values.mfiRemoteTokenSet).toBe(true);
  expect(isVisible(fields.find(field => field.key === 'mfiI2cPath')!, settings.values)).toBe(source === 'i2c');
  expect(isVisible(fields.find(field => field.key === 'mfiRemoteServer')!, settings.values)).toBe(source === 'remote');
  for (const language of ['en', 'zh-CN'] as const) {
    const t = (key: string) => translate(language, key);
    expect(mfiReadiness(settings.values, t)).not.toContain('mfi.readiness.');
    for (const status of ['ready', 'permission', 'missing', 'invalid', 'unsupported']) expect(mfiReadiness({ mfiSourceStatus: status }, t)).not.toContain('mfi.readiness.');
    expect(t('files.mfiPrivacyWarning')).not.toBe('files.mfiPrivacyWarning');
  }
});
it('submits a source with its parked settings and keeps tokens write-only', () => {
  const values = { mfiTarget: 'remote', mfiRemoteServer: 'https://mfi.example.org', mfiI2cPath: '/dev/i2c-1', mfiRemoteTokenSet: true };
  expect(mfiSubmission(values, 'mfiRemoteServer', 'https://new.example.org')).toEqual({ mfiTarget: 'remote', mfiI2cPath: '/dev/i2c-1', mfiRemoteServer: 'https://new.example.org' });
  expect(mfiSubmission(values, 'mfiRemoteToken', '')).toEqual({ mfiRemoteToken: '' });
  expect(mfiSubmission(values, 'output', 'web')).toBeNull();
  expect(mfiError(values)).toBeNull();
  expect(mfiError({ ...values, mfiRemoteServer: '' })).toBe('mfi.enterServer');
  expect(mfiError({ ...values, mfiRemoteServer: 'http://127.0.0.1:8080' })).toBeNull();
  for (const url of ['file:///tmp/server', 'https://user:password@example.org', 'https://example.org/#token']) expect(mfiError({ ...values, mfiRemoteServer: url })).toBe('mfi.enterServer');
  expect(mfiError({ ...values, mfiTarget: 'i2c', mfiI2cPath: '/tmp/device' })).toBe('mfi.enterPath');
  expect(mfiError({ ...values, mfiTarget: 'local', mfiRemoteServer: '' })).toBeNull();
  expect(publicNameConfigured({ privacyConfigAvailable: true, publicNamePrivacyAvailable: false, mfiRemoteTokenSet: true })).toBe(false);
});
