// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-27

import type { Status } from './api';
import type { Settings, Values } from './schema';
import type { Language } from './i18n';
import { optionLabel } from './schema';
import { esc } from './html';
type Translate = (key: string, args?: Record<string, string | number>) => string;
export interface ScreenSize { width: number; height: number }
export interface Screen extends ScreenSize { densityDpi: number }
export interface ScreenRecord { screen: Screen; size: ScreenSize; fullscreen: boolean; count: number; lastSeen: number }
export interface ScreenData {
  savedCanvas: Screen & { source: 'remembered' | 'built-in model' | 'default' };
  detected: Screen | null; builtIn: boolean; connected: boolean; enabled: boolean;
  records: ScreenRecord[];
  areas: (ScreenSize & { index: number; fullscreen: boolean; windowed: boolean })[];
  active: number; preference: boolean | null;
  switches: { time: number; index: number; accepted: boolean }[];
}
export const displayPresets = [[800, 480, 160], [1024, 600, 160], [1280, 720, 160], [1920, 720, 240], [1920, 1080, 240]] as const;
export function carSelectionErrors(v: Values): Record<string, string> {
  const errors: Record<string, string> = {};
  if (!v.carBrand || v.carBrand === 'other' && !String(v.carBrandText ?? '').trim()) errors.carBrand = 'car.brandRequired';
  if (!v.carModel || v.carModel === 'other' && !String(v.carModelText ?? '').trim()) errors.carModel = 'car.modelRequired';
  return errors;
}
export function trustedHttpsRecommended(v: Values): boolean {
  return v.carBrand === 'zeekr' && !(v.publicNameEnabled === true && !!v.publicName && !!v.publicNameProvider && v.publicNameTokenSet === true && v.publicNameAgreeTerms === true);
}
export function carNotes(v: Values, t: Translate): string {
  return Object.values(carSelectionErrors(v)).map(key => `<p class="error">${esc(t(key))}</p>`).join('') + (trustedHttpsRecommended(v) ? `<p class="notice">${esc(t('car.zeekrHttps'))} <a href="#settings/access/public-name">${esc(t('car.openHttps'))}</a></p>` : '');
}
function sameScreen(a: Screen, b: Screen): boolean { return a.width === b.width && a.height === b.height && a.densityDpi === b.densityDpi; }
export function screenRecords(data: ScreenData, full: boolean): ScreenRecord[] {
  return data.records.filter(r => r.fullscreen === full && data.detected && sameScreen(r.screen, data.detected))
    .sort((a, b) => b.count - a.count || b.lastSeen - a.lastSeen || a.size.width - b.size.width || a.size.height - b.size.height);
}
function object(value: unknown): Record<string, unknown> { return value && typeof value === 'object' && !Array.isArray(value) ? value as Record<string, unknown> : {}; }
// Android's contribution format allowlists capability fields, excluding tokens, URLs and network names.
export function screenShare(status: Status, settings: Settings): Record<string, unknown> | null {
  const data = status.screenData, screen = data?.detected;
  if (!data || !screen) return null;
  const json: Record<string, unknown> = { format: 2, app: status.version };
  for (const name of ['brand', 'brandText', 'model', 'modelText', 'year', 'yearText']) json[name] = Array.from(String(settings.values['car' + name[0].toUpperCase() + name.slice(1)] ?? '').replace(/[\u0000-\u001f\u007f-\u009f]/g, '')).slice(0, 40).join('').trim();
  json.screen = { width: screen.width, height: screen.height, densityDpi: screen.densityDpi };
  for (const [key, full] of [['fullscreen', true], ['windowed', false]] as const) json[key] = screenRecords(data, full).map(r => ({ width: r.size.width, height: r.size.height, count: r.count }));
  const info = object(status.receiverHistory?.screens.find(s => sameScreen(s, screen))?.info);
  if (typeof info.userAgent === 'string' && info.userAgent) json.userAgent = info.userAgent;
  for (const key of ['videoDecode', 'audioDecoder']) {
    if (!Array.isArray(info[key])) continue;
    json[key] = info[key].filter(r => r && typeof r === 'object' && !Array.isArray(r)).map(value => {
      const row = object(value), cleaned: Record<string, unknown> = {};
      for (const field of ['codec', 'width', 'height', 'preferHardware', 'preferSoftware', 'mediaSource', 'supported']) if (['string', 'number', 'boolean'].includes(typeof row[field])) cleaned[field] = row[field];
      for (const field of ['fps30', 'fps60']) if (row[field] && typeof row[field] === 'object') {
        const fps = object(row[field]); cleaned[field] = Object.fromEntries(['supported', 'smooth', 'powerEfficient'].filter(k => typeof fps[k] === 'boolean').map(k => [k, fps[k]]));
      }
      return cleaned;
    });
  }
  return json;
}
export function screenShareText(status: Status, settings: Settings, language: Language, t: Translate): string {
  const json = screenShare(status, settings);
  if (!json) return '';
  const labels = ['carBrand', 'carModel', 'carYear'].map(key => {
    const value = settings.values[key];
    if (value === 'other') return String(settings.values[key + 'Text'] || t('option.carYear.other'));
    const field = settings.schema.flatMap(s => s.fields).find(f => f.key === key);
    return optionLabel(field?.options?.find(o => o.value === value), language, t) || t('stage.noScreen');
  });
  return `${t('car.shareTitle')}\n${t('car.shareIdentity', { brand: labels[0], model: labels[1], year: labels[2] })}\n${JSON.stringify(json)}`;
}
const sizeText = (s: ScreenSize) => `${s.width} × ${s.height}`;
export function carScreenView(status: Status, t: Translate): string {
  const data = status.screenData;
  if (!data) return '';
  const saved = data.savedCanvas;
  const rows = [true, false].map(full => `<h3>${esc(t(full ? 'areas.fullscreen' : 'areas.window'))}</h3>${screenRecords(data, full).map(r => `<p>${esc(t('car.windowSize', { ...r.size, count: r.count }))}</p>`).join('') || `<p>${esc(t('car.noSizes'))}</p>`}`).join('');
  return `<details class="inset-group" open><summary>${esc(t('car.savedCanvas'))} · ${sizeText(saved)}</summary><p>${sizeText(saved)} · ${saved.densityDpi} dpi</p><p class="muted">${esc(t('car.source.' + saved.source))}</p><button data-screen-action="clear-saved-head-unit">${esc(t('car.clearSaved'))}</button></details><details class="inset-group" open><summary>${esc(t('car.detected'))}</summary>${data.builtIn ? `<p class="muted">${esc(t('car.builtIn'))}</p>` : ''}${data.detected ? `<p>${sizeText(data.detected)} · ${esc(t('car.pixelRatio', { ratio: (data.detected.densityDpi / 160).toFixed(2) }))}</p>${rows}` : `<p>${esc(t('car.noScreen'))}</p>`}</details><details class="inset-group"><summary>${esc(t('car.shareTitle'))}</summary><p>${esc(t('car.nextStart'))}</p><p>${esc(t('car.shareHint'))}</p><button data-screen-copy ${data.detected ? '' : 'disabled'}>${esc(t('car.copy'))}</button></details><p data-screen-result role="status"></p>`;
}
export function viewAreasView(status: Status, t: Translate): string {
  const data = status.screenData;
  if (!data) return '';
  const library = data.preference == null ? 'areas.all' : data.preference ? 'areas.fullscreen' : 'areas.window';
  const records = [true, false].map(full => `<h3>${esc(t(full ? 'areas.fullscreen' : 'areas.window'))}</h3>${data.records.filter(r => r.fullscreen === full).map(r => `<p class="mono">${sizeText(r.screen)} · ${(r.screen.densityDpi / 160).toFixed(2)} · ${esc(t('car.windowSize', { ...r.size, count: r.count }))} · ${esc(new Date(r.lastSeen).toLocaleString())}</p>`).join('') || `<p>${esc(t('car.noSizes'))}</p>`}`).join('');
  const exits = status.receiverHistory?.fullscreenExits ?? [];
  return `<details class="inset-group"><summary>${esc(t('field.carPlayViewAreas'))}</summary><p>${esc(t('areas.library', { library: t(library) }))}</p><h3>${esc(t('areas.declared'))}</h3>${data.enabled ? data.areas.map(a => `<p>${a.index}: ${sizeText(a)} · ${(a.width / a.height).toFixed(3)} · ${esc(t(a.index === 0 ? 'areas.canvas' : a.fullscreen && a.windowed ? 'areas.all' : a.fullscreen ? 'areas.fullscreen' : 'areas.window'))}</p><button data-show-area="${a.index}" ${status.running ? '' : 'disabled'} aria-pressed="${data.active === a.index}">${esc(t('areas.show', { index: a.index }))}</button>`).join('') : `<p>${esc(t('areas.empty'))}</p>`}${[...data.switches].reverse().map(s => `<p class="mono">${esc(new Date(s.time).toLocaleString())} · ${esc(t(s.accepted ? 'areas.queued' : 'areas.rejected', { index: s.index }))}</p>`).join('')}<h3>${esc(t('areas.exits'))}</h3>${exits.length ? [...exits].reverse().map(exit => {
    const report = object(exit.report), display = object(report.display), visible = object(display.viewport ?? report.viewport ?? display);
    const hidden = typeof report.hiddenMs === 'number' ? `${report.hiddenMs} ms` : t('areas.unknown');
    const size = typeof visible.width === 'number' && typeof visible.height === 'number' ? `${visible.width} × ${visible.height}` : t('areas.unknown');
    const restore = ['restored', 'denied', 'off'].includes(String(report.autoRestore)) ? String(report.autoRestore) : 'unknown';
    return `<p class="mono">${esc(new Date(exit.time).toLocaleString())} · ${esc(exit.client)}<br>${esc(t('areas.exitDetails', { hidden, size, restore: t('areas.' + restore) }))}</p>`;
  }).join('') : `<p>${esc(t('areas.noExits'))}</p>`}<h3>${esc(t('areas.records'))}</h3><button data-screen-action="clear-window-records">${esc(t('areas.clear'))}</button>${records}<p data-screen-result role="status"></p></details>`;
}
export function displayPresetsView(t: Translate): string {
  return `<div class="inset-group"><p>${esc(t('display.presets'))}</p>${displayPresets.map((p, i) => `<button data-display-preset="${i}">${p[0]} × ${p[1]} · ${p[2]} dpi</button>`).join('')}</div>`;
}
