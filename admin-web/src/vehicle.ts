// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-27

import { request } from './api';
import { confirmSheet, patchMarkup } from './dom';
import { esc } from './html';
import { TracePoller } from './carlink-trace';
import type { Translate } from './stage';
export interface VehicleReport { kind: string; fields: Record<string, string | number | boolean | null>; receivedAt: number }
export interface VehicleSnapshot { id: number; time: number; reports: VehicleReport[] }
export interface VehicleView { sample?: boolean; enabled: boolean; sensorActive: boolean; malformedFrames: number; storageFailed: boolean; reports: VehicleReport[]; history: VehicleSnapshot[] }
const labels: Record<string, string> = { Gps: 'gps', Lights: 'lights', Gear: 'gear', Fuel: 'fuel', Gyroscope: 'gyroscope', Acceleration: 'acceleration', LightSensor: 'light', DayNight: 'daynight', height: 'altitude', antennaState: 'antenna', satellitesUsed: 'used', satellitesVisible: 'visible', lowBeam: 'low', highBeam: 'high', positionLamp: 'position', reverseLamp: 'reverse', brakeLamp: 'brake', maxLux: 'max', minLux: 'min', currentLux: 'current', gyroscopeType: 'type' };
export function vehicleState(view: VehicleView): 'off' | 'waiting' | 'data' { return !view.enabled ? 'off' : view.reports.length ? 'data' : 'waiting'; }
export function vehiclePanel(view: VehicleView, t: Translate, now = Date.now(), open: ReadonlySet<string> = new Set()): string {
  const value = (raw: string | number | boolean | null) => {
    const key = String(raw);
    return raw == null ? '—' : ['true', 'false', 'DAY', 'NIGHT', 'UNKNOWN', 'SINGLE', 'THREE_AXIS'].includes(key) ? t(`vehicle.${({ true: 'on', false: 'off_value', DAY: 'day', NIGHT: 'night', UNKNOWN: 'unknown', SINGLE: 'single_axis', THREE_AXIS: 'three_axes' } as Record<string, string>)[key]}`) : key;
  };
  const signals = (reports: VehicleReport[], at: number) => reports.slice(0, 8).map(report => `<section class="inset-group" data-key="${esc(report.kind)}"><h2>${esc(t(`vehicle.${labels[report.kind] ?? report.kind}`))}</h2>${Object.entries(report.fields).slice(0, 16).map(([key, raw]) => `<div class="inset-row"><span>${esc(t(`vehicle.${labels[key] ?? key}`))}</span><span class="copy-text">${esc(value(raw))}</span></div>`).join('')}<p class="muted">${esc(t('vehicle.updated', { seconds: Math.max(0, Math.floor((at - report.receivedAt) / 1000)) }))}</p></section>`).join('');
  const state = vehicleState(view);
  return `${view.sample ? `<p>${esc(t('vehicle.sample'))}</p>` : ''}<p>${esc(t(state === 'data' ? 'vehicle.title' : `vehicle.${state}`))}</p><p class="muted">${esc(t('vehicle.privacy'))}</p>${state === 'off' ? `<button data-vehicle-enable>${esc(t('vehicle.enable'))}</button>` : ''}${state === 'data' ? `<p class="muted">${esc(t('vehicle.units'))}</p>${signals(view.reports, now)}` : ''}${view.storageFailed ? `<p class="error">${esc(t('vehicle.storage_error'))}</p>` : ''}<section class="inset-group"><h2>${esc(t('vehicle.history'))}</h2><p class="muted">${esc(t('vehicle.history_hint'))}</p>${view.history.length || view.storageFailed ? `<button data-vehicle-clear>${esc(t('vehicle.clear'))}</button>` : `<p>${esc(t('vehicle.history_empty'))}</p>`}${view.history.slice(0, 30).map(snapshot => `<details data-snapshot="${snapshot.id}" ${open.has(String(snapshot.id)) ? 'open' : ''}><summary>${esc(new Date(snapshot.time).toLocaleString())}</summary>${signals(snapshot.reports, snapshot.time)}</details>`).join('')}</section>`;
}
export function mountVehicle(container: HTMLElement, t: Translate, errorKey: (error: unknown) => string): () => void {
  container.insertAdjacentHTML('beforeend', '<div data-vehicle-content></div><p role="alert" class="error" data-vehicle-error></p>');
  const content = container.querySelector<HTMLElement>('[data-vehicle-content]')!;
  const error = container.querySelector<HTMLElement>('[data-vehicle-error]')!;
  let disposed = false, busy = false;
  const poller = new TracePoller(async () => {
    try {
      const view = await request<VehicleView>('/vehicle');
      if (disposed) return;
      const open = new Set(Array.from(content.querySelectorAll<HTMLDetailsElement>('details[open]')).map(d => d.dataset.snapshot!));
      patchMarkup(content, vehiclePanel(view, t, Date.now(), open));
      content.querySelectorAll<HTMLButtonElement>('button').forEach(button => { button.disabled = busy; });
    } catch (e) { if (!disposed) error.textContent = t(errorKey(e)); }
  });
  content.addEventListener('click', async event => {
    const button = (event.target as HTMLElement).closest<HTMLButtonElement>('button');
    if (busy || !button) return;
    const clear = button.hasAttribute('data-vehicle-clear');
    if (!clear && !button.hasAttribute('data-vehicle-enable')) return;
    if (clear && !await confirmSheet(`${t('vehicle.clear_question')} ${t('vehicle.clear_hint')}`, t('vehicle.clear'), t('ui.cancel'))) return;
    busy = true; button.disabled = true; error.textContent = '';
    try {
      if (clear) await request('/actions/clear-vehicle-history', 'POST', {});
      else await request('/settings', 'PUT', { vehicleInformation: true });
    } catch (e) { if (!disposed) error.textContent = t(errorKey(e)); }
    finally { busy = false; if (!disposed) void poller.refresh(); }
  });
  poller.start();
  return () => { disposed = true; poller.dispose(); };
}

/** A session-only developer preview; it never changes the collection preference. */
export function mountVehicleSample(container: HTMLElement, t: Translate, errorKey: (error: unknown) => string): () => void {
  container.insertAdjacentHTML('beforeend', `<label><input type="checkbox" data-vehicle-sample>${esc(t('vehicle.sampleControl'))}</label><p class="muted">${esc(t('vehicle.sampleHint'))}</p><a href="#settings/vehicle">${esc(t('vehicle.sampleOpen'))}</a><p class="error" role="alert" data-sample-error></p>`);
  const toggle = container.querySelector<HTMLInputElement>('[data-vehicle-sample]')!;
  const error = container.querySelector<HTMLElement>('[data-sample-error]')!;
  let disposed = false, busy = false;
  const poller = new TracePoller(async () => {
    try { const view = await request<VehicleView>('/vehicle'); if (!disposed && !busy) toggle.checked = view.sample === true; }
    catch (e) { if (!disposed) error.textContent = t(errorKey(e)); }
  });
  toggle.addEventListener('change', async () => {
    if (busy) return;
    busy = true; toggle.disabled = true; error.textContent = '';
    try { await request('/vehicle/sample', 'POST', { enabled: toggle.checked }); }
    catch (e) { if (!disposed) error.textContent = t(errorKey(e)); }
    finally { busy = false; if (!disposed) { toggle.disabled = false; void poller.refresh(); } }
  });
  poller.start();
  return () => { disposed = true; poller.dispose(); };
}
