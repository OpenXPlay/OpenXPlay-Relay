// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-16

import { expect, it } from 'vitest';
import { hotspotForm, validateHotspot, restartRequired, restartView } from '../src/hotspot-settings';
import { translate } from '../src/i18n';
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
