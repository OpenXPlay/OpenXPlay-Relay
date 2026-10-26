// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-23

import { api, ApiError, NetworkError, type Status, type NetworkInterface, type NetworkPlan, type NetworkProposal, type PlannedRole } from './api';
import { COUNTRY_CODES } from './wifi-countries';
import type { NetworkRolesSetting } from './schema';
import { networkView, networkCheckMessage } from './network-diagnostics';
type Translate = (key: string, params?: Record<string, string | number>) => string;
import { esc } from './html';
export function interfaceLabel(i: NetworkInterface): string {
  const bus = i.bus === 'usb' ? 'USB' : i.bus;
  const product = i.product || i.driver;
  const bands = i.wifi?.bands.filter(b => b.stationChannels.length || b.apChannels.length).map(b => b.band).join('/');
  return [i.name, product ? `${product} (${bus})` : bus, bands ? `${bands} GHz` : ''].filter(Boolean).join(' · ');
}
const selected = (s: Status, role: PlannedRole) => s.network.interfaces?.find(i => i.name === role.interface?.name);
const label = (s: Status, role: PlannedRole) => { const i = selected(s, role); return i ? interfaceLabel(i) : role.interface?.name || role.interface?.ref || '—'; };
export function checksView(proposal: Pick<NetworkProposal, 'checks'>, t: Translate): string {
  return `<ul class="network-checks">${proposal.checks.map(c => `<li class="network-check-${esc(c.level)}">${esc(t(`roles.level.${c.level}`))}: ${esc(networkCheckMessage(c, t))}</li>`).join('')}</ul>`;
}
export function connectionsView(s: Status, t: Translate): string {
  const p = s.network.plan;
  if (s.platform !== 'linux' || !p || !s.network.interfaces) return '';
  const waiting = (role: PlannedRole) => role.missing ? ` · ${t('connections.waiting', { interface: role.interface?.name || role.interface?.ref || '—' })}` : '';
  const iphone = `${t('connections.bluetooth', { adapter: s.hostProfile?.bluetoothAdapter?.name || '—' })} · Wi-Fi: ${t(p.iphone.link === 'hotspot' ? 'connections.hotspot' : 'connections.saved', { interface: label(s, p.iphone) })}${p.iphone.link === 'hotspot' ? ` · ${p.iphone.band} GHz · ${t('roles.channel', { channel: p.iphone.channel })}` : ''}`;
  const browser = p.carBrowser.network === 'iphone' ? t('roles.iphoneNetwork') : `${t('connections.interfaceNetwork', { interface: label(s, p.carBrowser) })} · ${selected(s, p.carBrowser)?.ipv4.join(', ') || '—'}`;
  const carlink = `${t('connections.carlink', { interface: label(s, p.carlink) })} · ${t(p.carlink.followOver ? 'connections.follow' : 'connections.noFollow')}`;
  const admin = `${t(({ off: 'roles.off', failed: 'bt.state.failed', 'waiting-interface': 'roles.missing', 'paused-carlink': 'roles.timeShared' } as Record<string, string>)[s.adminHotspot?.state || (p.adminHotspot.enabled ? 'waiting-interface' : 'off')] || 'connections.admin.up')} · ${label(s, p.adminHotspot)} · ${p.adminHotspot.ssid} · ${p.adminHotspot.band} GHz · ${t('roles.channel', { channel: p.adminHotspot.channel })} · https://openxplay-linux.local:6975 · https://10.79.0.1:6975`;
  const roles: [string, PlannedRole, string][] = [['iphone', p.iphone, iphone], ['carBrowser', p.carBrowser, browser], ...(['carlink', 'both'].includes(s.output) ? [['carlink', p.carlink, carlink] as [string, PlannedRole, string]] : []), ['adminHotspot', p.adminHotspot, admin]];
  return `<section class="card settings-section connections" data-section="connections"><h2>${t('connections.title')}</h2><span class="mode-badge">${t(`connections.${p.source}`)}</span><dl class="facts">${roles.map(([name, role, detail]) => `<dt>${t(`roles.${name}`)}</dt><dd>${esc(detail + waiting(role))}</dd>`).join('')}</dl>${s.network.probing ? `<p role="status">${t('connections.probing')}</p>` : checksView({ checks: s.network.checks || [] }, t)}<button id="connections-edit" ${s.network.probing ? 'disabled' : ''}>${t('ui.edit')}</button><details class="interface-inventory"><summary>${t('roles.interfaces')}</summary>${networkView(s, t)}</details></section>`;
}
export function roleDraft(s: Status, saved?: NetworkRolesSetting): NetworkRolesSetting {
  const p = s.network.plan!;
  const ref = (role: PlannedRole) => role.interface?.mac || role.interface?.ref || '';
  return { mode: p.source, iphone: { link: p.iphone.link, interface: ref(p.iphone) }, carBrowser: { network: p.carBrowser.network, interface: p.carBrowser.network === 'iphone' ? null : ref(p.carBrowser) }, carlink: { interface: ref(p.carlink) },
    adminHotspot: saved?.adminHotspot || { enabled: false, interface: null, ssid: 'OpenXPlay-Admin', band: 'auto', channel: 0 } };
}
const adminChanged = (before: NetworkPlan, after: NetworkPlan) => JSON.stringify(before.adminHotspot) !== JSON.stringify(after.adminHotspot);
export function ownAddressWarning(s: Status, next: NetworkPlan, hostname: string): boolean {
  if (['openxplay-linux.local', s.publicName?.name, s.publicName?.trustedName].includes(hostname) && s.network.plan?.adminHotspot.enabled && adminChanged(s.network.plan, next)) return true;
  if (!/^(\d{1,3}\.){3}\d{1,3}$/.test(hostname) || hostname.split('.').some(n => Number(n) > 255)) return false;
  const own = s.network.interfaces?.find(i => i.ipv4.includes(hostname));
  const old = s.network.plan;
  if (!own || !old) return false;
  if (old.adminHotspot.interface?.name === own.name && adminChanged(old, next)) return true;
  return (['iphone', 'carBrowser', 'carlink'] as const).some(key => (old[key].interface?.name === own.name || next[key].interface?.name === own.name) && (old[key].interface?.name !== next[key].interface?.name || old[key].missing !== next[key].missing
    || (key === 'iphone' && old.iphone.link !== next.iphone.link) || (key === 'carBrowser' && old.carBrowser.network !== next.carBrowser.network)));
}
export function proposalView(s: Status, proposal: NetworkProposal, hostname: string, t: Translate): string {
  return checksView(proposal, t) + `<div class="connection-changes" role="status">${proposal.changes.map(c => `<p>${esc(t(`connections.change.${c}`))}</p>`).join('') || `<p>${t('connections.noChanges')}</p>`}</div>`
    + ((ownAddressWarning(s, proposal.plan, hostname) || (['10.79.0.1', 'openxplay-linux.local', s.publicName?.trustedName, s.publicName?.name].includes(hostname) && proposal.changes.some(c => c === 'adminHotspot' || c === 'wifiCountry'))) ? `<p class="notice" role="alert">${t('connections.ownAddress')}</p>` : '');
}
export const canSaveRoles = (proposal: NetworkProposal | null, busy: boolean) => !busy && !!proposal && !proposal.checks.some(c => c.level === 'error');
type RoleKey = 'iphone' | 'carBrowser' | 'carlink';
export function pick(draft: NetworkRolesSetting, key: RoleKey, value: string): NetworkRolesSetting {
  const next = structuredClone(draft);
  if (key === 'carBrowser') next.carBrowser = { network: value === 'iphone' ? 'iphone' : 'interface', interface: value === 'iphone' ? null : value };
  else if (key === 'iphone') next.iphone!.interface = value;
  else next.carlink = { interface: value };
  return next;
}
function adminRadio(s: Status, draft: NetworkRolesSetting): NetworkInterface | undefined {
  const ref = draft.adminHotspot?.interface || s.network.plan?.adminHotspot.interface?.name;
  return s.network.interfaces?.find(i => i.mac === ref || i.name === ref);
}
export function adminChannels(s: Status, draft: NetworkRolesSetting): number[] {
  const radio = adminRadio(s, draft)?.wifi;
  const band = draft.adminHotspot?.band === 'auto' ? radio?.bands.some(b => b.band === '5' && b.apChannels.length) ? '5' : '2.4' : draft.adminHotspot?.band;
  return radio?.bands.find(b => b.band === band)?.apChannels || [];
}
const channelOptions = (channels: number[], current: number, t: Translate) => [0, ...new Set([...channels, ...(current ? [current] : [])])].map(c => `<option value="${c}"${current === c ? ' selected' : ''}>${c || t('connections.auto')}</option>`).join('');
export function adminForm(s: Status, draft: NetworkRolesSetting, t: Translate): string {
  const a = draft.adminHotspot!;
  const interfaces = (s.network.interfaces || []).filter(i => i.wifi?.ap);
  const options = [{ value: '', label: t('connections.auto') }, ...interfaces.map(i => ({ value: i.mac || i.name, label: interfaceLabel(i) }))];
  if (a.interface && !options.some(i => i.value === a.interface)) options.push({ value: a.interface, label: t('connections.waiting', { interface: a.interface }) });
  return `<fieldset><legend>${t('roles.adminHotspot')}</legend><label for="admin-enabled"><input id="admin-enabled" type="checkbox"${a.enabled ? ' checked' : ''}>${t('roles.adminHotspot')}</label><label for="admin-interface">${t('roles.interfaces')}</label><select id="admin-interface">${options.map(i => `<option value="${esc(i.value)}"${i.value === (a.interface || '') ? ' selected' : ''}>${esc(i.label)}</option>`).join('')}</select><label for="admin-ssid">${t('field.hotspotSsid')}</label><input id="admin-ssid" maxlength="32" value="${esc(a.ssid)}"><label for="admin-password">${t('field.adminHotspotPassword')} · ${s.network.plan?.adminHotspot.passwordSet ? t('roles.passwordSet') : ''}</label><input id="admin-password" type="password" autocomplete="new-password" minlength="8" maxlength="63" placeholder="${t('networks.keep')}"><label for="admin-band">${t('field.hotspotBand')}</label><select id="admin-band">${['auto', '2.4', '5'].map(b => `<option value="${b}"${a.band === b ? ' selected' : ''}>${b === 'auto' ? t('connections.auto') : `${b} GHz`}</option>`).join('')}</select><label for="admin-channel">${t('field.wifiChannel')}</label><select id="admin-channel">${channelOptions(adminChannels(s, draft), a.channel, t)}</select></fieldset>`;
}
export function countryOptions(country: string, t: Translate): string {
  return ['system', '00', ...COUNTRY_CODES].map(code => `<option value="${code}"${country === code ? ' selected' : ''}>${code === 'system' || code === '00' ? t(`country.${code}`) : code}</option>`).join('');
}
export function connectionsForm(s: Status, draft: NetworkRolesSetting, t: Translate): string {
  const options = (key: RoleKey) => {
    const current = key === 'carBrowser' ? draft.carBrowser?.network === 'iphone' ? 'iphone' : draft.carBrowser?.interface : draft[key]?.interface;
    const interfaces = (s.network.interfaces || []).filter(i => key === 'carBrowser' || i.kind === 'wifi');
    const entries = interfaces.map(i => ({ value: i.mac || i.name, label: interfaceLabel(i) }));
    if (key === 'carBrowser') entries.unshift({ value: 'iphone', label: t('roles.iphoneNetwork') });
    if (current && !entries.some(i => i.value === current)) entries.push({ value: current, label: t('connections.waiting', { interface: current }) });
    return entries.map(i => `<option value="${esc(i.value)}"${i.value === current ? ' selected' : ''}>${esc(i.label)}</option>`).join('');
  };
  return `<h2 id="connections-title">${t('connections.title')}</h2><form id="connections-form"><label for="roles-mode">${t('connections.mode')}</label><select id="roles-mode">${['auto', 'custom'].map(mode => `<option value="${mode}"${draft.mode === mode ? ' selected' : ''}>${t(`connections.${mode}`)}</option>`).join('')}</select><fieldset id="custom-roles"${draft.mode === 'auto' ? ' hidden' : ''}><legend>${t('connections.roles')}</legend><label for="roles-link">${t('connections.iphoneLink')}</label><select id="roles-link">${['hotspot', 'saved-network'].map(link => `<option value="${link}"${draft.iphone?.link === link ? ' selected' : ''}>${t(`roles.link.${link}`)}</option>`).join('')}</select>${(['iphone', 'carBrowser', 'carlink'] as const).map(key => `<label for="roles-${key}">${t(`roles.${key}`)}</label><select id="roles-${key}" data-role="${key}">${options(key)}</select>`).join('')}</fieldset>${adminForm(s, draft, t)}<label for="wifi-country">${t('field.wifiCountry')}</label><select id="wifi-country">${countryOptions(s.network.country?.configured || 'system', t)}</select><p>${t('connections.countryProbe')}</p><div id="roles-preview" aria-live="polite"></div><p id="roles-error" class="error" role="alert"></p><div class="dialog-actions"><button type="button" data-close>${t('ui.cancel')}</button><button id="roles-save" type="submit" class="primary" disabled>${t('ui.save')}</button></div></form>`;
}
export function editConnections(s: Status, saved: NetworkRolesSetting | undefined, t: Translate, done: () => Promise<void>, trigger: HTMLElement): void {
  let draft = roleDraft(s, saved);
  let proposal: NetworkProposal | null = null;
  let generation = 0;
  let busy = false;
  let wifiCountry = s.network.country?.configured || 'system';
  let password = '';
  const extra = () => ({ wifiCountry, ...(password ? { adminHotspotPassword: password } : {}) });
  const dialog = document.createElement('dialog'); dialog.setAttribute('aria-labelledby', 'connections-title');
  dialog.innerHTML = connectionsForm(s, draft, t);
  document.body.append(dialog); dialog.showModal();
  const close = () => { generation++; dialog.close(); dialog.remove(); trigger.focus(); };
  dialog.querySelector('[data-close]')!.addEventListener('click', close);
  dialog.addEventListener('cancel', event => { event.preventDefault(); close(); });
  const save = dialog.querySelector<HTMLButtonElement>('#roles-save')!;
  const error = dialog.querySelector<HTMLElement>('#roles-error')!;
  const preview = async () => {
    const current = ++generation;
    proposal = null; save.disabled = true; error.textContent = '';
    dialog.querySelector<HTMLElement>('#custom-roles')!.hidden = draft.mode !== 'custom';
    try {
      const next = await api.previewRoles(draft, extra());
      if (current !== generation || !dialog.isConnected) return;
      proposal = next;
      const channels = adminChannels({ ...s, network: { ...s.network, plan: next.plan } }, draft);
      dialog.querySelector('#admin-channel')!.innerHTML = channelOptions(channels, draft.adminHotspot!.channel, t);
      dialog.querySelector('#roles-preview')!.innerHTML = proposalView(s, next, location.hostname, t);
      // Validate alternatives through the same endpoint; impossible choices keep their reason.
      if (draft.mode === 'custom') await Promise.all([...dialog.querySelectorAll<HTMLSelectElement>('[data-role]')].map(async select => {
        const key = select.dataset.role as RoleKey;
        await Promise.all([...select.options].map(async option => {
          const candidate = await api.previewRoles(pick(draft, key, option.value), extra());
          if (current !== generation || !dialog.isConnected) return;
          const errors = candidate.checks.filter(c => c.level === 'error' && c.roles.includes(key));
          option.disabled = errors.length > 0;
          option.textContent = `${option.textContent!.split(' — ')[0]}${errors.length ? ` — ${errors.map(c => networkCheckMessage(c, t)).join(' ')}` : ''}`;
        }));
      }));
      if (current === generation) save.disabled = !canSaveRoles(proposal, busy);
    } catch (e) {
      if (current === generation) { proposal = null; save.disabled = true; if (e instanceof ApiError && e.checks.length) dialog.querySelector('#roles-preview')!.innerHTML = checksView({ checks: e.checks }, t); else error.textContent = t(e instanceof ApiError ? e.key : 'error.network'); }
    }
  };
  dialog.querySelector('form')!.addEventListener('change', event => {
    const select = event.target as HTMLSelectElement;
    if (select.id === 'roles-mode') draft.mode = select.value as 'auto' | 'custom';
    else if (select.id === 'roles-link') draft.iphone!.link = select.value as 'hotspot' | 'saved-network';
    else if (select.id === 'wifi-country') wifiCountry = select.value;
    else if (select.id === 'admin-password') password = select.value;
    else if (select.id === 'admin-enabled') draft.adminHotspot!.enabled = (select as unknown as HTMLInputElement).checked;
    else if (select.id === 'admin-ssid') draft.adminHotspot!.ssid = select.value;
    else if (select.id === 'admin-interface' || select.id === 'admin-band') {
      if (select.id === 'admin-interface') draft.adminHotspot!.interface = select.value || null;
      else draft.adminHotspot!.band = select.value as 'auto' | '2.4' | '5';
      draft.adminHotspot!.channel = 0;
      dialog.querySelector<HTMLSelectElement>('#admin-channel')!.innerHTML = channelOptions(adminChannels(s, draft), 0, t);
    }
    else if (select.id === 'admin-channel') draft.adminHotspot!.channel = Number(select.value);
    else if (select.dataset.role) draft = pick(draft, select.dataset.role as RoleKey, select.value);
    void preview();
  });
  dialog.querySelector('form')!.addEventListener('submit', async event => {
    event.preventDefault();
    if (!canSaveRoles(proposal, busy)) return;
    busy = true; save.disabled = true;
    dialog.querySelectorAll<HTMLInputElement | HTMLSelectElement>('select, input').forEach(select => { select.disabled = true; });
    try { await api.saveRoles(draft, extra()); close(); await done(); }
    catch (e) {
      // A reconnect can lose the response after the save. Never repeat the PUT.
      if (e instanceof NetworkError) { close(); await done(); return; }
      const failure = t(e instanceof ApiError ? e.key : 'error.request');
      busy = false;
      dialog.querySelectorAll<HTMLInputElement | HTMLSelectElement>('select, input').forEach(select => { select.disabled = false; });
      await preview();
      error.textContent = failure;
    }
  });
  void preview();
}
