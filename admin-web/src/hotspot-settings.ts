// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-16

import { icon } from './icons';
import type { Status, Wireless, HotspotSecurity } from './api';
type Translate = (key: string, parameters?: Record<string, string | number>) => string;
export function validateHotspot(ssid: string, password: string, keepPassword = false): string {
  const bytes = new TextEncoder().encode(ssid).length;
  if (bytes < 1 || bytes > 32) return 'error.hotspotSsid';
  if (!(keepPassword && password === '') && !/^[\x20-\x7e]{8,63}$/.test(password)) return 'error.hotspotPassword';
  return '';
}
import { esc } from './html';
export function hotspotBandSelect(wireless: Wireless | undefined, t: Translate): string {
  const band = wireless?.hotspot.configuredBand ?? 'auto';
  const automatic = wireless?.hotspot.autoBand ? t('hotspot.automatic', {
    band: t(`option.hotspotBand.${wireless.hotspot.autoBand}`), preset: t(`option.devicePreset.${wireless.preset.id}`),
  }) : t('option.hotspotBand.auto');
  return `<label for="hotspot-band">${t('field.hotspotBand')}</label><select id="hotspot-band"${wireless?.ap5 === false ? ' aria-describedby="hotspot-band-hint"' : ''}>${['auto', '2.4', '5'].map(value => `<option value="${value}"${value === band ? ' selected' : ''}${value === '5' && wireless?.ap5 === false ? ' disabled' : ''}>${esc(value === 'auto' ? automatic : t(`option.hotspotBand.${value}`))}</option>`).join('')}</select>${wireless?.ap5 === false ? `<p id="hotspot-band-hint" class="muted">${t('hotspot.no5')}</p>` : ''}`;
}
export function stationBandHint(status: Pick<Status, 'platform' | 'wireless'>, t: Translate): string {
  return status.platform === 'linux' && status.wireless?.station5 === false ? `<p>${t('wifi.only24')}</p>` : '';
}
export function devicePresetLabel(value: string, status: Pick<Status, 'platform' | 'wireless'> | null, t: Translate): string {
  return value === 'auto' && status?.platform === 'linux' && status.wireless
    ? t('preset.automatic', { label: t(`option.devicePreset.${status.wireless.preset.detected}`) })
    : t(`option.devicePreset.${value}`);
}
export function hotspotSecuritySelect(security: HotspotSecurity, supported: HotspotSecurity[], t: Translate): string {
  const values: HotspotSecurity[] = ['open', 'wpa2', 'wpa3-transition', 'wpa3'];
  return `<label for="hotspot-security">${t('field.hotspotSecurity')}</label><select id="hotspot-security">${values.filter(value => supported.includes(value) || value === security).map(value => `<option value="${value}"${value === security ? ' selected' : ''}${supported.includes(value) ? '' : ' disabled'}>${t(`option.hotspotSecurity.${value}`)}</option>`).join('')}</select>`;
}
export function hotspotForm(ssid: string, t: Translate, wireless?: Wireless, hotspot?: Status['hotspot']): string {
  return `<form novalidate><label for="hotspot-name">${t('net.name')}</label><input id="hotspot-name" value="${esc(ssid)}" autocomplete="off" aria-describedby="hotspot-error"><label for="hotspot-password">${t('net.password')}</label><div class="input-row"><input id="hotspot-password" type="password" autocomplete="new-password" placeholder="${hotspot ? t('networks.keep') : ''}" aria-describedby="hotspot-error"><button id="hotspot-reveal" type="button" class="icon-button" aria-label="${t('ui.reveal')}" aria-pressed="false">${icon('eye')}</button></div>${hotspot ? hotspotSecuritySelect(hotspot.security ?? 'wpa2', hotspot.supportedSecurity?.length ? hotspot.supportedSecurity : ['open', 'wpa2'], t) : ''}${hotspotBandSelect(wireless, t)}<p class="muted">${t('hotspot.format')}</p><p id="hotspot-error" class="error" role="alert"></p><div class="dialog-actions"><button type="button" data-close>${t('ui.cancel')}</button><button type="submit" class="primary">${t('hotspot.review')}</button></div></form>`;
}
export function restartRequired(notices: string[], applied?: string): boolean {
  return applied === 'next-start' || notices.includes('notice.nextStart');
}
export function restartView(required: boolean, t: (key: string) => string): string {
  return required ? `<p role="status">${t('ui.restartRequired')}</p><button id="restart-now">${t('ui.restartNow')}</button><p id="restart-error" class="error" role="alert"></p>` : '';
}
