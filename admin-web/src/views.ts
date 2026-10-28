// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-16

import type { Status, WifiNetwork } from './api';
import { homeView, addressViews, iphoneReason } from './logic';
import { icon } from './icons';
import { formatRemaining } from './pairing';
import { carlinkStatsView } from './video-stats';
import { videoLine } from './diagnostics';
type Translate = (key: string, parameters?: Record<string, string | number>) => string;
import { esc } from './html';
export function statusView(s: Status, t: Translate, defaultPassword = false) {
  const v = homeView(s, undefined, defaultPassword);
  const link = (connected: boolean, label: string) => `<span class="journey-link ${connected ? 'connected' : 'waiting'}"><span class="sr-only">${t(label)}: ${t(connected ? 'ui.connected' : 'home.disconnected')}</span></span>`;
  return `<article class="card hero ${v.tone}"><h1 id="hero-status" role="status" aria-live="polite">${t(v.hero)}</h1>${v.bluetooth === 'failed' ? `<p class="error" role="alert">${t('bt.failed')}</p>` : v.bluetooth ? `<p class="notice" role="status">${t('bt.recovering')}</p>` : ''}<div class="journey">${icon('smartphone')}${link(v.phoneConnected, 'ui.iphone')}${icon('router')}${link(v.carConnected, 'ui.screen')}${icon('monitor')}</div>${v.action === 'car' && v.phoneConnected ? `<p class="muted">${t('home.phoneConnected')}</p>` : v.hero === 'home.phone' ? `<p class="muted">${t(s.pairingAvailable ? 'home.nearby' : 'home.pairOnDevice')}</p>` : v.promoteAddresses ? `<p class="muted">${t('home.readyHint')}</p>` : ''}${s.network.followingCarlink ? `<p>${t('network.followingCarlink')}</p>` : ''}<div id="context-action"></div>${s.platform === 'linux' && s.autoStartPaused ? `<button id="home-start">${t('ui.start')}</button>` : ''}<p id="pair-countdown" role="timer">${v.seconds ? t('home.pairCountdown', { time: formatRemaining(v.seconds) }) : ''}</p>${v.seconds ? '' : s.pairingNotified ? `<p role="status">${t('pairing.notified')}</p>` : s.pairingNeedsPermission ? `<p role="status">${t('pairing.permission')}</p>` : s.pairingNeedsConfirmation ? `<p role="status">${t('pairing.confirm')}</p>` : s.pairingOnRelay ? `<p role="status">${t('pairing.settingsOpen')}</p>` : ''}<p id="action-result" class="error" role="alert"></p>${s.notices.map(n => `<p class="notice">${esc(t(n))}</p>`).join('')}</article>`;
}
export function addressesView(s: Status, _developer: boolean, t: Translate) {
  const addresses = addressViews(s, false);
  const adminUrls = s.platform === 'linux' ? s.adminUrls : undefined;
  const copy = (value: string) => `<button class="icon-button" data-copy="${esc(value)}" aria-label="${t('ui.copy')}: ${esc(value)}" title="${t('ui.copy')}">${icon('copy')}<span class="copy-feedback" role="status"></span></button>`;
  return `<article class="card addresses ${homeView(s).promoteAddresses ? 'promoted' : ''}"><h2>${t('home.addresses')}</h2>${s.hostProfile && !s.hostProfile.wifiPresent ? `<p class="notice" role="alert">${esc(t('host.wifiMissing', { interface: s.hostProfile.wifiInterface }))}</p>` : ''}${s.network.mode !== 'shared-wifi' && s.hotspot ? s.screenConnected === true ? `<p class="muted">${esc(t('net.joinedHotspot', { ssid: s.hotspot.ssid }))}</p>` : `<p>${esc(t('net.joinHotspot', { ssid: s.hotspot.ssid }))}</p>${hotspotDetails(s, t)}` : ''}${addresses.length > 1 ? `<label class="sr-only" for="address-network">${t('ui.network')}</label><select id="address-network">${addresses.map((a, i) => `<option value="${i}">${esc(t(a.label))} · ${esc(a.ipv4)}</option>`).join('')}</select>` : ''}${addresses.map((a, i) => `<section class="network-address" data-address="${i}" ${i ? 'hidden' : ''}><span class="muted">${esc(t(a.label))}</span><div class="address-row"><div><span class="copy-text ip" tabindex="0">${esc(a.ipv4)}</span><small class="muted">${esc(a.host)}</small></div>${copy(a.ipv4)}</div><div class="address-row receiver-address"><div><strong>${t('settings.screen')}</strong><a class="copy-text" href="${esc(a.receiver)}" target="_blank" rel="noopener">${esc(a.receiver)}</a>${a.fallback ? `<small class="muted">${esc(t('publicName.fallback', { address: a.fallback }))}</small>` : ''}</div>${copy(a.receiver)}</div>${adminUrls ? '' : `<div class="address-row admin-address"><div><span class="muted">${t('ui.admin')}</span><a class="copy-text" href="${esc(a.admin)}" target="_blank" rel="noopener">${esc(a.admin)}</a></div>${copy(a.admin)}</div>`}</section>`).join('') || `<p>${t('ui.noUrls')}</p>`}${adminUrls ? adminUrls.map(url => `<div class="address-row admin-address"><div><span class="muted">${t('home.adminPage')}</span><a class="copy-text" href="${esc(url)}" target="_blank" rel="noopener">${esc(url)}</a></div>${copy(url)}</div>`).join('') : ''}</article>`;
}
export function devicesView(s: Status, _developer: boolean, t: Translate) {
  const v = homeView(s);
  const reason = iphoneReason(s.iphone.reason);
  const state = (connected: boolean, unpaired: boolean) => {
    const name = connected ? 'connected' : unpaired ? 'unpaired' : 'disconnected';
    return `<span class="device-state ${name}"><span class="status-dot" aria-hidden="true">${connected ? '✓' : unpaired ? '?' : '−'}</span>${t(connected ? 'ui.connected' : unpaired ? 'carlink.unpaired' : 'home.disconnected')}</span>`;
  };
  return `<article class="card devices"><h2>${t('home.devices')}</h2><details><summary>${icon('smartphone')}<span>${esc(s.iphone.name || t('ui.iphone'))}</span>${state(v.phoneConnected, ['none', 'unpaired'].includes(reason))}</summary><p>${esc(t(`reason.${iphoneReason(s.iphone.reason)}`))}</p><a href="#settings">${t('ui.choosePhone')}</a></details><details><summary>${icon('monitor')}<span>${esc(s.carlink?.headUnit?.name || t('ui.screen'))}</span>${state(v.carConnected, ['carlink', 'both'].includes(s.output) && (s.carlink?.trusted === false || s.carlink?.state === 'unpaired'))}</summary><p>${t(v.carConnected ? 'ui.connected' : 'home.disconnected')}</p>${s.platform === 'linux' ? carlinkStatsView(s.carlink, t) : ''}${s.carlink?.lastError ? `<p class="error">${esc(t(s.carlink.lastError))} ${t('home.retryCar')}</p>` : ''}</details>${v.active && s.video ? `<p class="muted video-line">${esc(videoLine(s, t))}</p>` : ''}</article>`;
}

export function hotspotDetails(s: Status, t: Translate): string {
  if (!s.hotspot) return '';
  const copy = (value: string, label: string) => `<button type="button" class="icon-button" data-copy="${esc(value)}" aria-label="${t('ui.copy')}: ${t(label)}">${icon('copy')}<span class="copy-feedback" role="status" aria-live="polite"></span></button>`;
  const w = s.platform === 'linux' ? s.wireless : undefined;
  const wireless = w ? `<p class="muted">${esc(t('hotspot.radio', { band: t(`option.hotspotBand.${w.hotspot.band}`), channel: w.hotspot.channel }))}</p>${w.mode === 'unknown' ? '' : `<p class="muted">${t(`wireless.mode.${w.mode}`)}</p>`}` : '';
  return `<div class="hotspot-details"><span class="muted">${t('net.name')}</span><div class="address-row"><span class="copy-text" tabindex="0">${esc(s.hotspot.ssid)}</span>${copy(s.hotspot.ssid, 'net.name')}</div>${s.hotspot.security === 'open' ? `<p>${t('option.hotspotSecurity.open')}</p>` : `<span class="muted">${t('net.password')}</span><div class="address-row"><span class="copy-text" data-secret="${esc(s.hotspot.password)}" tabindex="0">••••••••</span><button type="button" class="icon-button" data-secret-toggle aria-pressed="false" aria-label="${t('ui.reveal')}">${icon('eye')}</button>${copy(s.hotspot.password, 'net.password')}</div>`}${wireless}</div>`;
}

export function carlinkConnectView(s: Status, connectPin: string, connectBusy: boolean, carlinkError: string, t: Translate): string {
  const c = s.carlink;
  return `<div class="carlink-card">${c?.detail === 'carlink.pinStale' || c?.lastError ? `<p class="error">${esc(t(c?.detail === 'carlink.pinStale' ? c.detail : c!.lastError!))}</p>` : ''}${homeView(s).carRows.map(row => `<p>${t(row.label)}: ${esc(row.value)}</p>`).join('')}<form id="carlink-connect-form"><label for="carlink-connect-pin">${t('carlink.pin')}</label><input id="carlink-connect-pin" type="text" inputmode="numeric" pattern="[0-9]{4,8}" minlength="4" maxlength="8" autocomplete="off" value="${esc(connectPin)}" ${c?.trusted ? '' : 'required'} aria-describedby="carlink-connect-hint"><p id="carlink-connect-hint" class="muted">${t('carlink.pinHint')}</p><button class="primary" ${connectBusy ? 'disabled' : ''}>${t('carlink.connect')}</button></form><p id="carlink-result" class="error" role="alert">${esc(t(carlinkError))}</p></div>`;
}

export function nearbyNetworksView(networks: WifiNetwork[], t: Translate): string {
  return networks.map((network, index) => {
    const signal = Math.max(0, Math.min(100, Math.round(network.signal)));
    const bars = Math.ceil(signal / 25);
    return `<button type="button" class="phone-row" data-nearby="${index}"><span>${esc(network.ssid)}</span><span class="network-indicators"><span class="signal-bars" role="img" aria-label="${esc(t('wifi.signal', { signal }))}">${[1, 2, 3, 4].map(bar => `<i aria-hidden="true" class="${bar <= bars ? 'filled' : ''}"></i>`).join('')}</span>${network.secure ? `<span role="img" aria-label="${esc(t('wifi.secured'))}">${icon('lock')}</span>` : ''}</span></button>`;
  }).join('');
}
