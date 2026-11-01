// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-31

import { it, expect } from 'vitest';
import { addressesView } from '../src/views';
import { translate } from '../src/i18n';
import ready from './fixtures/linux/status-ready.json';
import type { Status } from '../src/api';

it('lists advertised per-network admin names in both languages and preserves Android URLs', () => {
  const status = structuredClone(ready) as Status;
  status.adminUrls = ['https://openxplay-car-2.local:6975/', 'https://openxplay-admin-3.local:6975/'];
  for (const language of ['en', 'zh-CN'] as const) {
    const t = (key: string, parameters?: Record<string, string | number>) => translate(language, key, parameters);
    const html = addressesView(status, false, t);
    for (const url of status.adminUrls) expect(html).toContain(`href="${url}"`);
    expect(html).toContain(t('admin.carNetwork'));
    expect(html).toContain(t('admin.otherNetwork'));
    expect(html).not.toContain('openxplay-linux.local');
    status.platform = 'android';
    expect(addressesView(status, false, t)).not.toContain('openxplay-car-2.local');
    status.platform = 'linux';
  }
});
