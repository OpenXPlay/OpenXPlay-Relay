// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-16

import type { Settings, Value } from './schema';
import type { Language } from './i18n';
import { esc } from './html';
export function enabledPlugins(value: Value): string[] {
  return Array.isArray(value) ? value.filter((id): id is string => typeof id === 'string') : [];
}
export function togglePlugin(value: Value, id: string, checked: boolean): string[] {
  const ids = enabledPlugins(value);
  return checked ? ids.includes(id) ? ids : [...ids, id] : ids.filter(item => item !== id);
}
export function pluginList(settings: Settings, language: Language, t: (key: string) => string, developer = false): string {
  const plugins = settings.installedPlugins ?? [];
  const enabled = enabledPlugins(settings.values.pluginsEnabled);
  if (!plugins.length) return `<p class="muted">${esc(t('plugins.empty'))}</p>`;
  return plugins.map(plugin => {
    const name = plugin.name[language] || plugin.name[language.split('-')[0]] || plugin.name.en || plugin.id;
    const image = plugin.icon && /^data:image\/(?:png|svg\+xml),/.test(plugin.icon) ? `<img class="plugin-icon" src="${esc(plugin.icon)}" alt="">` : '';
    return `<div class="plugin-row"><label class="switch-row">${image}<span><strong>${esc(name)}</strong><small class="muted plugin-meta">${esc(plugin.version)} · ${esc(plugin.license)}</small>${plugin.nonCommercialAssets ? `<span class="tag">${esc(t('plugins.nc'))}</span>` : ''}${developer ? `<small class="muted plugin-meta">${esc(plugin.id)}</small>` : ''}</span><input type="checkbox" role="switch" data-plugin="${esc(plugin.id)}" ${enabled.includes(plugin.id) ? 'checked' : ''}><span class="switch-track" aria-hidden="true"></span></label></div>`;
  }).join('');
}
