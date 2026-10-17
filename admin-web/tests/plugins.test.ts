// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-16

import { expect, it } from 'vitest';
import { pluginList, togglePlugin } from '../src/plugins';
import { settingsViews } from '../src/logic';
import { controlFor, type Settings } from '../src/schema';
import { translate } from '../src/i18n';
const settings: Settings = {
  schema: [{ id: 'plugins', titleKey: 'section.plugins', fields: [
    { key: 'plugins', type: 'bool', labelKey: 'field.plugins', hintKey: 'hint.plugins' },
    { key: 'pluginsEnabled', type: 'plugins', labelKey: 'field.pluginsEnabled' },
  ] }], values: { plugins: false, pluginsEnabled: ['org.test.pet'] },
  installedPlugins: [{ id: 'org.test.pet', name: { en: 'Pet <cat>', 'zh-CN': '宠物' },
    version: '1.0.0', license: '(MIT OR Apache-2.0) AND CC-BY-NC-SA-4.0',
    icon: 'data:image/svg+xml,%3Csvg%2F%3E', nonCommercialAssets: true }],
};
it('keeps plugins in their own user section in both modes', () => {
  for (const developer of [false, true]) {
    const views = settingsViews(settings, developer);
    expect(views.find(v => v.id === 'plugins')?.fields).toEqual(settings.schema[0].fields);
    expect(views.filter(v => v.id !== 'plugins').flatMap(v => v.fields)).toEqual([]);
  }
  expect(controlFor(settings.schema[0].fields[1])).toBe('plugins');
});
it('renders localized metadata, safe icons, switches, NC badge and developer IDs', () => {
  const en = pluginList(settings, 'en', key => translate('en', key));
  expect(en).toContain('Pet &lt;cat&gt;');
  expect(en).toContain('1.0.0'); expect(en).toContain('CC-BY-NC-SA-4.0');
  expect(en).toContain('Non-commercial assets'); expect(en).toContain('checked');
  expect(en).toContain('src="data:image/svg+xml,');
  expect(en).not.toContain('plugin-meta">org.test.pet');
  const zh = pluginList(settings, 'zh-CN', key => translate('zh-CN', key), true);
  expect(zh).toContain('宠物'); expect(zh).toContain('非商业用途素材');
  expect(zh).toContain('plugin-meta">org.test.pet');
  const fallback = structuredClone(settings);
  delete fallback.installedPlugins![0].name['zh-CN'];
  fallback.installedPlugins![0].icon = 'https://example.com/icon.png';
  expect(pluginList(fallback, 'zh-CN', key => key)).toContain('Pet &lt;cat&gt;');
  expect(pluginList(fallback, 'zh-CN', key => key)).not.toContain('<img');
});
it('keeps selection order, removes IDs and never adds duplicates', () => {
  expect(togglePlugin(['org.b', 'org.a'], 'org.c', true)).toEqual(['org.b', 'org.a', 'org.c']);
  expect(togglePlugin(['org.b', 'org.a'], 'org.b', true)).toEqual(['org.b', 'org.a']);
  expect(togglePlugin(['org.b', 'org.a'], 'org.b', false)).toEqual(['org.a']);
  expect(togglePlugin(null, 'org.a', true)).toEqual(['org.a']);
});
it('explains offline browser execution and CarPlay in both languages', () => {
  for (const lang of ['en', 'zh-CN'] as const) {
    for (const key of ['section.plugins', 'field.plugins', 'field.pluginsEnabled', 'hint.plugins', 'plugins.empty', 'plugins.nc', 'error.plugins'])
      expect(translate(lang, key)).not.toBe(key);
    expect(translate(lang, 'hint.plugins')).toContain('CarPlay');
  }
  expect(pluginList({ schema: [], values: {} }, 'en', key => translate('en', key))).toContain('No plugins are installed.');
});
