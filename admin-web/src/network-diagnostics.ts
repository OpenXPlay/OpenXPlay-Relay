// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-22

import type { Status, NetworkCheck, PlannedRole } from './api';
type Translate = (key: string, params?: Record<string, string | number>) => string;
const esc = (s: unknown) => String(s ?? '').replace(/[&<>"']/g, c => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' })[c]!);
export const NETWORK_CHECK_CODES = ['interface-missing', 'no-ap-mode', 'no-ap-channel', 'roles-conflict', 'time-shared', 'follow-over', 'carlink-band-gap', 'carlink-no-5ghz', 'ap-spacing', 'admin-hotspot-nat'];
export function networkCheckMessage(check: NetworkCheck, t: Translate): string {
  const params = Object.fromEntries(Object.entries(check.params).map(([key, value]) => [key, Array.isArray(value) ? value.join(', ') : String(value ?? '')]));
  return t(`roles.check.${check.code}`, { ...params, roles: check.roles.map(role => t(`roles.${role}`)).join(', '), interfaces: check.interfaces.join(', ') || '—' });
}
export function networkView(status: Status, t: Translate): string {
  const network = status.network;
  if (status.platform !== 'linux' || !network.plan || !network.interfaces) return '';
  const support = (value: boolean | undefined) => t(value === undefined ? 'host.unknown' : value ? 'host.yes' : 'host.no');
  const headers = ['name', 'product', 'bus', 'mac', 'kind', 'bands', 'modes', 'concurrency', 'activity', 'ipv4'];
  const rows = network.interfaces.map(i => {
    const w = i.wifi;
    const activity = [i.operstate, t(`roles.activity.${i.activity.kind}`), i.activity.profile, i.activity.ssid,
      i.activity.channel ? t('roles.channel', { channel: i.activity.channel }) : '', i.activity.width].filter(Boolean).join(' · ');
    const cells = [i.name, i.product || i.driver || '—', i.bus, `${i.mac || '—'} (${t(`roles.mac.${i.macSource}`)})`, t(`roles.kind.${i.kind}`),
      w ? w.bands.map(b => `${b.band} GHz`).join(', ') : '—',
      w ? `AP: ${support(w.ap)} · ${t('host.station')}: ${support(w.station)} · P2P-client: ${support(w.p2pClient)} · P2P-GO: ${support(w.p2pGo)}` : '—',
      w ? t(`roles.concurrency.${w.concurrency}`) : i.kind === 'wifi' ? t('host.unknown') : '—', activity, i.ipv4.join(', ') || '—'];
    return `<tr>${cells.map(cell => `<td>${esc(cell)}</td>`).join('')}</tr>`;
  }).join('');
  const p = network.plan;
  const selected = (role: PlannedRole) => role.interface ? [role.interface.name || role.interface.ref, role.interface.mac].filter(Boolean).join(' · ') : '—';
  const band = (role: { band: string; channel: number }) => `${role.band} GHz · ${t('roles.channel', { channel: role.channel })}`;
  const plans: [string, PlannedRole, string][] = [
    ['iphone', p.iphone, `${t(`roles.link.${p.iphone.link}`)} · ${selected(p.iphone)}${p.iphone.link === 'hotspot' ? ` · ${band(p.iphone)}` : ''}`],
    ['carBrowser', p.carBrowser, p.carBrowser.network === 'iphone' ? t('roles.iphoneNetwork') : selected(p.carBrowser)],
    ['carlink', p.carlink, `${selected(p.carlink)} · ${t(p.carlink.followOver ? 'roles.followOver' : 'roles.separateRadio')}`],
    ['adminHotspot', p.adminHotspot, p.adminHotspot.enabled ? `${selected(p.adminHotspot)} · ${p.adminHotspot.ssid} · ${band(p.adminHotspot)}${p.adminHotspot.timeSharedWith ? ` · ${t('roles.timeShared')}` : ''} · ${t('roles.passwordSet')}: ${support(p.adminHotspot.passwordSet)}` : t('roles.off')],
  ];
  const country = network.country;
  const countries = [country?.global ? `global: ${country.global}` : '', ...(country?.phys ?? []).map(p => `${p.phy}: ${p.country || '—'}${p.selfManaged ? ` (${t('roles.selfManaged')})` : ''}`)].filter(Boolean).join(' · ') || t('host.unknown');
  return `<section class="network-diagnostics"><h3>${t('roles.interfaces')}</h3><p>${esc(t('roles.probedAt', { time: network.probedAt ? new Date(network.probedAt * 1000).toISOString() : t('roles.pending') }))}</p>`
    + `<p>${esc(t('roles.country', { configured: country?.configured || '00', effective: countries }))}</p>`
    + `<div class="network-table"><table><thead><tr>${headers.map(h => `<th scope="col">${t(`roles.column.${h}`)}</th>`).join('')}</tr></thead><tbody>${rows}</tbody></table></div>`
    + `<h3>${t('roles.plan')}</h3><dl class="facts">${plans.map(([name, role, detail]) => `<dt>${t(`roles.${name}`)}</dt><dd>${esc(detail)} · ${esc(t(`roles.source.${role.source}`))}${role.missing ? ` · ${t('roles.missing')}` : ''}</dd>`).join('')}</dl>`
    + (network.checks?.length ? `<ul>${network.checks.map(c => `<li class="network-check-${esc(c.level)}">${esc(t(`roles.level.${c.level}`))}: ${esc(networkCheckMessage(c, t))}</li>`).join('')}</ul>` : `<p>${t('roles.noChecks')}</p>`) + '</section>';
}
