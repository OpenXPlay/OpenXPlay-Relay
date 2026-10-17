// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-16

import { icon } from './icons';
export function validateHotspot(ssid: string, password: string): string {
  const bytes = new TextEncoder().encode(ssid).length;
  if (bytes < 1 || bytes > 32) return 'error.hotspotSsid';
  if (!/^[\x20-\x7e]{8,63}$/.test(password)) return 'error.hotspotPassword';
  return '';
}
const esc = (value: string) => value.replace(/[&<>"']/g, c => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' })[c]!);
export function hotspotForm(ssid: string, t: (key: string) => string): string {
  return `<form novalidate><label for="hotspot-name">${t('net.name')}</label><input id="hotspot-name" value="${esc(ssid)}" autocomplete="off" aria-describedby="hotspot-error"><label for="hotspot-password">${t('net.password')}</label><div class="input-row"><input id="hotspot-password" type="password" autocomplete="new-password" aria-describedby="hotspot-error"><button id="hotspot-reveal" type="button" class="icon-button" aria-label="${t('ui.reveal')}" aria-pressed="false">${icon('eye')}</button></div><p class="muted">${t('hotspot.format')}</p><p id="hotspot-error" class="error" role="alert"></p><div class="dialog-actions"><button type="button" data-close>${t('ui.cancel')}</button><button type="submit" class="primary">${t('hotspot.review')}</button></div></form>`;
}
export function restartRequired(notices: string[], applied?: string): boolean {
  return applied === 'next-start' || notices.includes('notice.nextStart');
}
export function restartView(required: boolean, t: (key: string) => string): string {
  return required ? `<p role="status">${t('ui.restartRequired')}</p><button id="restart-now">${t('ui.restartNow')}</button><p id="restart-error" class="error" role="alert"></p>` : '';
}
