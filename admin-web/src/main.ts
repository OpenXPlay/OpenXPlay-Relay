// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-15

import { constraintKey, connectionConstraint, networkOptionState, wifiDirectDisabled, needsHotspotConfirmation } from './carlink-connection';
import { pluginList, togglePlugin } from './plugins';
import { validateHotspot, hotspotForm, restartRequired, restartView } from './hotspot-settings';
import './style.css';
import { carlinkConnectView, statusView, addressesView, devicesView, hotspotDetails, nearbyNetworksView } from './views';
import { homeView, settingsViews, developerPreference, networkPanelView, switchConsequences, savedNetworks, savedNetworkTags, addSavesAndConnects, canConnectNetwork } from './logic';
import { putSharedNetwork, deleteSharedNetwork, sharedNetworkUpdate, sharedNetworkPassword } from './shared-networks';
import type { SavedNetwork } from './schema';
import { api, ApiError, NetworkError, events, type SessionState, type Status, type Phone, type WifiNetwork } from './api';
import { isVisible, coerceValue, validatePassword, type Field, type Settings, type Value, type Illustration } from './schema';
import { defaultLanguage, translate, type Language } from './i18n';
import { sessionRoute, wifiGuidance, sortNetworks, wifiJoinCredentials, carlinkView, canConnectCarlink, validCarlinkPin } from './logic';
import { icon } from './icons';
import { anchorPairing, createCountdown, formatRemaining, pairingPanelView } from './pairing';
import { copyText, diagnosticsView, receiverReport } from './diagnostics';
import { publicNamePanel } from './public-name';

const root = document.querySelector<HTMLDivElement>('#app')!;
let language: Language = defaultLanguage(navigator.language);
const t = (key: string, parameters?: Record<string, string | number>) => translate(language, key, parameters);
const esc = (value: unknown): string => String(value ?? '').replace(/[&<>"']/g, char => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' })[char]!);
let developer = developerPreference(location.search, () => localStorage.getItem('openxplay.developer'));
let session: SessionState | null = null;
let defaultPassword = false;
let currentView = location.hash === '#settings' ? 'settings' : 'home';
window.addEventListener('hashchange', () => { currentView = location.hash === '#settings' ? 'settings' : 'home'; render(); });
function showSettings(target?: string) {
  currentView = 'settings'; history.replaceState(null, '', '#settings'); render();
  if (target) document.getElementById(target)?.focus();
}
let handoff: { action: 'forget'; ssid: string; error?: string } | null = null;
let networks: WifiNetwork[] = [];
let switching: { target: string; consequences: ReturnType<typeof switchConsequences>; started: number } | null = null;
let networkAnnouncement = '';
let needsRestart = false;
let networkError = '';
let status: Status | null = null;
let settings: Settings | null = null;
let offline = false;
let epoch = 0;
let polling = false;
let actionBusy = false;
let pageError = '';
let carlinkError = '';
let connectPin = '';
let connectBusy = false;
let dismissedPin: number | null = null;
let pairingBusy = false;
let pairingError = '';
let publicNameBusy = false;
let publicNameMessage = '';
let reportCopied: { client: string; key: string; until: number } | null = null;
const copied = new Map<string, { key: string; until: number }>();
const phoneNames = new Map<string, string>();
const feedback = new Map<string, string>();
const pending = new Map<string, { value: Value; revision: number; timer?: ReturnType<typeof setTimeout>; busy: boolean }>();
const fields = () => settings ? settingsViews(settings, developer).flatMap(section => section.fields) : [];
const fieldId = (key: string) => `field-${encodeURIComponent(key)}`;
function result(key: string, message: string) {
  feedback.set(key, message);
  const el = document.getElementById(`${fieldId(key)}-result`);
  document.getElementById(fieldId(key))?.setAttribute('aria-invalid', String(!!message && !message.startsWith('ui.')));
  if (el) { el.textContent = t(message); el.classList.toggle('error', message.startsWith('error.') || (!message.startsWith('ui.') && message !== '')); }
}
function banner() {
  const el = document.getElementById('connection');
  if (el) { el.hidden = !offline || !!handoff || !!switching; el.textContent = t('ui.reconnecting'); }
}
events.addEventListener('offline', () => { offline = true; banner(); });
events.addEventListener('online', () => { offline = false; banner(); });
function resetSession(next: SessionState) {
  epoch++;
  for (const item of pending.values()) clearTimeout(item.timer);
  needsRestart = false; pending.clear(); feedback.clear(); networks = []; switching = null; networkError = ''; networkAnnouncement = ''; status = null; settings = null; carlinkError = ''; connectPin = ''; connectBusy = false; dismissedPin = null; pairingBusy = false; pairingError = ''; reportCopied = null; publicNameBusy = false; publicNameMessage = '';
  document.querySelectorAll('dialog').forEach(dialog => { dialog.close(); dialog.remove(); });
  session = next; actionBusy = false; render();
}
events.addEventListener('unauthorized', () => { if (session === 'ready' && !handoff) resetSession('login'); });
// Diagnostics only feed the developer section; the pairing deadline moves onto this browser's clock.
async function fetchStatus(): Promise<Status> { return anchorPairing(await api.status(developer), Date.now() / 1000, status?.pairingDeadline ?? null); }
function errorKey(error: unknown): string { return error instanceof ApiError ? error.key : error instanceof NetworkError ? 'error.network' : 'error.request'; }
function shell() {
  document.documentElement.lang = language;
  root.innerHTML = `<header><div class="brand">${icon('router')}<div><strong id="relay-name">${esc(status?.name ?? 'OpenXPlay Relay')}</strong><span id="relay-host" class="muted">${esc(status?.host ?? location.host)}</span></div></div><div class="header-actions">${session === 'ready' ? `<nav class="view-tabs" aria-label="${t('home.navigation')}"><a href="#home" ${currentView === 'home' ? 'aria-current="page"' : ''}>${t('home.title')}</a><a href="#settings" ${currentView === 'settings' ? 'aria-current="page"' : ''}>${t('ui.settings')}</a></nav>` : ''}<details class="more-menu"><summary aria-label="${t('home.more')}">⋯</summary><div class="menu-items"><label class="sr-only" for="language">${t('ui.language')}</label><select id="language"><option value="en" ${language === 'en' ? 'selected' : ''}>${t('ui.english')}</option><option value="zh-CN" ${language === 'zh-CN' ? 'selected' : ''}>${t('ui.chinese')}</option></select>${session === 'ready' ? `<button id="logout" class="quiet">${icon('log-out')}<span>${t('ui.logout')}</span></button>` : ''}${session === 'ready' ? `<button id="menu-password">${t('ui.changePassword')}${defaultPassword ? `<small class="muted">${t('home.passwordNote')}</small>` : ''}</button>${status?.platform === 'linux' ? `<button id="menu-car">${t('carlink.forget')}</button>` : ''}${developer && currentView === 'settings' ? `<button id="menu-restart">${t('home.restart')}</button>` : ''}` : ''}</div></details></div></header><div id="connection" role="status" class="banner" ${offline ? '' : 'hidden'}>${t('ui.reconnecting')}</div><div id="restart-required" class="card" hidden></div><main id="main"></main>`;
  document.querySelectorAll('.menu-items button').forEach(button => button.addEventListener('click', () => { document.querySelector<HTMLDetailsElement>('.more-menu')!.open = false; }));
  document.getElementById('menu-password')?.addEventListener('click', () => showSettings('current-password'));
  document.getElementById('menu-car')?.addEventListener('click', confirmForgetCarlink);
  document.getElementById('menu-restart')?.addEventListener('click', async () => { if (!window.confirm(t('home.restartHint'))) return; try { await api.action('restart'); void tick(); } catch (e) { pageError = errorKey(e); updateDashboard(); } });
  document.getElementById('language')!.addEventListener('change', event => {
    language = (event.target as HTMLSelectElement).value as Language; render(); document.getElementById('language')!.focus();
  });
  document.getElementById('logout')?.addEventListener('click', async () => {
    try { await api.logout(); resetSession('login'); } catch (error) { pageError = errorKey(error); updateDashboard(); }
  });
}
function render() {
  shell();
  if (switching) renderSwitching();
  else if (handoff) renderHandoff();
  else if (session === 'login') renderAuth();
  else if (session === 'ready' && settings && status) renderReady();
  else document.getElementById('main')!.innerHTML = `<p role="status">${esc(t(pageError || 'ui.loading'))}</p>`;
}
function renderAuth() {
  document.getElementById('main')!.innerHTML = `<section class="card auth"><div class="eyebrow">${t('ui.admin')}</div><h1>${t('ui.login')}</h1><p class="muted">${t('ui.loginHint')}</p>${defaultPassword ? `<p>${t('ui.firstTime')}</p>` : ''}<form id="auth-form"><label for="password">${t('ui.password')}</label><input id="password" name="password" type="password" autocomplete="current-password" required aria-describedby="auth-error"><p id="auth-error" class="error" role="alert"></p><button class="primary" type="submit">${t('ui.signIn')}</button></form></section>`;
  document.getElementById('auth-form')!.addEventListener('submit', async event => {
    event.preventDefault();
    const form = event.target as HTMLFormElement;
    const button = form.querySelector('button')!; button.disabled = true;
    try {
      await api.login((form.elements.namedItem('password') as HTMLInputElement).value);
      const next = await api.session(); defaultPassword = next.defaultPassword; session = sessionRoute(next); await loadReady();
    } catch (error) {
      document.getElementById('auth-error')!.textContent = error instanceof ApiError && error.status === 429 ? t('error.rateLimit', { seconds: error.retryAfter }) : t(errorKey(error));
    } finally { button.disabled = false; }
  });
}

async function loadReady() {
  const current = epoch;
  const [nextStatus, nextSettings, phones] = await Promise.all([fetchStatus(), api.settings(), api.phones()]);
  if (current !== epoch || session !== 'ready') return;
  phones.forEach(phone => phoneNames.set(phone.id, phone.name));
  status = nextStatus; settings = nextSettings; pageError = ''; render();
}
function renderReady() {
  renderRestart();
  document.getElementById('main')!.innerHTML = currentView === 'home' ? '<div id="dashboard"></div>' : `<section aria-label="${t('ui.settings')}"><div id="settings"></div></section>`;
  if (currentView === 'home') { updateDashboard(); return; }
  const container = document.getElementById('settings')!;
  container.innerHTML = settingsViews(settings!, developer).map(section => `<section class="card settings-section" data-section="${esc(section.id)}"><h2>${esc(t(section.titleKey))}</h2>${section.fields.filter(f => !['networkMode', 'sharedWifiNetworks'].includes(f.key)).map(fieldMarkup).join('') + (section.id === 'connection' ? '<div id="pairing-panel"></div><div id="network-panel"></div>' : section.id === 'publicName' ? '<div id="public-name-panel"></div>' : '')}</section>`).join('');
  for (const field of fields().filter(f => !['networkMode', 'sharedWifiNetworks'].includes(f.key))) bindField(field);
  renderNetworkPanel(); renderPairingPanel();
  updateVisibility(); renderPublicName();
  container.insertAdjacentHTML('beforeend', `<section class="card" id="password-card"><h2>${t('settings.security')}</h2><form id="password-form">${['current', 'new', 'confirm'].map(name => `<label for="${name}-password">${t(`ui.${name}Password`)}</label><input id="${name}-password" name="${name}" type="password" autocomplete="${name === 'current' ? 'current-password' : 'new-password'}" ${name === 'current' ? '' : 'minlength="8"'} required>`).join('')}<p class="muted">${t('error.passwordFormat')}</p><button class="primary">${t('ui.changePassword')}</button><p id="password-result" role="status"></p></form></section>`);
  container.insertAdjacentHTML('beforeend', `<section class="card"><h2>${t('settings.about')}</h2><p>${esc(status!.name)}</p><label class="switch-row"><span>${t('settings.developerOptions')}</span><input id="developer-options" type="checkbox" role="switch" ${developer ? 'checked' : ''}><span class="switch-track" aria-hidden="true"></span></label>${developer ? `<p>${esc(status!.version)}</p><pre id="developer-state">${esc(status!.state)} · ${esc(status!.iphone.reason)}</pre><button id="developer-start">${t(status!.running ? 'ui.stop' : 'ui.start')}</button>` : ''}</section>`);
  document.getElementById('developer-options')!.addEventListener('change', event => { developer = (event.target as HTMLInputElement).checked; try { localStorage.setItem('openxplay.developer', developer ? '1' : '0'); } catch {} render(); });
  document.getElementById('developer-start')?.addEventListener('click', async () => { try { await api.action(status!.running ? 'stop' : 'start'); void tick(); } catch (e) { pageError = errorKey(e); updateDashboard(); } });
  if (developer) {
    const options = settings!.schema.flatMap(s => s.fields).find(f => f.key === 'networkMode')?.options ?? [];
    const section = container.querySelector('[data-section="developer"]') ?? container.appendChild(document.createElement('section'));
    section.classList.add('card');
    section.insertAdjacentHTML('beforeend', `<div class="field"><label for="developer-network-mode">${t('field.networkMode')}</label><select id="developer-network-mode">${options.map(o => `<option value="${esc(o.value)}" ${networkOptionState(status?.platform, settings!.values, o.value)} ${o.value === status!.network.mode ? 'selected' : ''}>${esc(t(o.labelKey))}</option>`).join('')}</select>${connectionConstraint(status?.platform, t)}<button id="forget-all-wifi">${t('net.forgetAll')}</button></div>`);
    document.getElementById('developer-network-mode')!.addEventListener('change', event => { const select = event.target as HTMLSelectElement; const target = select.value; select.value = status!.network.mode; requestNetworkSwitch(target); });
    document.getElementById('forget-all-wifi')!.addEventListener('click', () => void forgetAllWifi());
  }
  if (developer) { container.insertAdjacentHTML('beforeend', `<section class="card" id="diagnostics-card"><h2>${t('diag.title')}</h2><div id="diagnostics"></div></section>`); renderDiagnostics(); }
  bindPassword();
}
function updateDashboard() {
  const el = document.getElementById('dashboard');
  if (!el || !status) return;
  const s = status;
  const raw = document.getElementById('developer-state'); if (raw) raw.textContent = `${s.state} · ${s.iphone.reason}`;
  // Per-second receiver statistics belong to the developer section only.
  const { receivers: _receivers, ...summary } = s;
  const signature = JSON.stringify([summary, developer, language, pageError, carlinkError, connectBusy, actionBusy, homeView(s).seconds > 0, defaultPassword]);
  if (el.dataset.signature === signature) { syncPinDialog(); return; }
  el.dataset.signature = signature;
  const openDetails = [...el.querySelectorAll<HTMLDetailsElement>('.devices details')].map(d => d.open);
  const addressNetwork = el.querySelector<HTMLSelectElement>('#address-network')?.value;
  const focusedCopy = (document.activeElement as HTMLElement | null)?.dataset.copy;

  const name = document.getElementById('relay-name'); if (name) name.textContent = s.name;
  const host = document.getElementById('relay-host'); if (host) host.textContent = s.host;
  // Poll only updates dashboard DOM; setting inputs retain their caret and focus.
  const focused = document.activeElement?.id;
  const selection = document.activeElement instanceof HTMLInputElement ? [document.activeElement.selectionStart, document.activeElement.selectionEnd] : null;
  const view = homeView(s, undefined, defaultPassword);
  el.innerHTML = statusView(s, t, defaultPassword) + addressesView(s, developer, t) + devicesView(s, developer, t);
  document.getElementById('action-result')!.textContent = pageError ? t(pageError) : '';
  const action = document.getElementById('context-action')!;
  if (view.action === 'car') renderCarlink();
  else if (view.action === 'pair' && view.seconds > 0) {
    // While the window is open the countdown replaces the primary button; closing it is the only action.
    if (s.pairingCancelable) {
      action.innerHTML = `<button id="context-cancel" ${actionBusy ? 'disabled' : ''}>${t('pairing.cancel')}</button>`;
      document.getElementById('context-cancel')!.addEventListener('click', () => void pairingAction('cancel-pairing'));
    }
  } else if (view.action === 'pair' && (s.pairingNeedsConfirmation || s.pairingOnRelay || s.pairingNeedsPermission || s.pairingNotified)) { /* The relay's own screen takes over; nothing to press here. */ }
  else if (view.action) {
    action.innerHTML = `<button id="context-primary" class="primary" ${actionBusy || view.seconds > 0 ? 'disabled' : ''}>${t(view.action === 'wifi' ? 'home.setupWifi' : view.action === 'password' ? 'home.setPassword' : 'home.pair')}</button>`;
    document.getElementById('context-primary')!.addEventListener('click', async () => {
      if (view.action === 'password') { showSettings('current-password'); return; }
      if (view.action === 'wifi') {
        showSettings();
        editSharedNetwork(null);
        return;
      }
      void pairingAction('pair-iphone');
    });
  }
  el.querySelectorAll<HTMLDetailsElement>('.devices details').forEach((d, i) => { d.open = openDetails[i] ?? false; });
  syncPinDialog();
  el.querySelector<HTMLSelectElement>('#address-network')?.addEventListener('change', event => {
    const value = (event.target as HTMLSelectElement).value;
    el.querySelectorAll<HTMLElement>('[data-address]').forEach(section => { section.hidden = section.dataset.address !== value; });
  });
  const networkSelect = el.querySelector<HTMLSelectElement>('#address-network');
  if (networkSelect && addressNetwork && networkSelect.querySelector(`option[value="${CSS.escape(addressNetwork)}"]`)) { networkSelect.value = addressNetwork; networkSelect.dispatchEvent(new Event('change')); }
  bindCopies(el);
  if (focusedCopy) el.querySelector<HTMLButtonElement>(`[data-copy="${CSS.escape(focusedCopy)}"]`)?.focus();
  if (focused && el.querySelector(`[id="${CSS.escape(focused)}"]`)) {
    const element = document.getElementById(focused); element?.focus();
    if (element instanceof HTMLInputElement && selection?.[0] != null && selection[1] != null) element.setSelectionRange(selection[0], selection[1]);
  }
}
async function pairingAction(name: 'pair-iphone' | 'cancel-pairing') {
  if (actionBusy) return;
  const current = epoch;
  actionBusy = true; pairingBusy = true; pairingError = ''; pageError = ''; updateDashboard(); renderPairingPanel();
  try { await api.action(name); const next = await fetchStatus(); if (current === epoch) status = next; }
  catch (error) { if (current === epoch) pageError = pairingError = errorKey(error); }
  finally {
    if (current === epoch) {
      actionBusy = false; pairingBusy = false; updateDashboard(); renderPairingPanel();
      (document.getElementById(name === 'pair-iphone' ? 'pairing-cancel' : 'pairing-open') ?? document.getElementById('context-cancel') ?? document.getElementById('context-primary'))?.focus();
    }
  }
}
function renderPairingPanel() {
  const el = document.getElementById('pairing-panel'); if (!el || !status) return;
  const s = status;
  const signature = JSON.stringify([homeView(s).seconds > 0, s.pairingAvailable, s.pairingCancelable, s.pairingNeedsConfirmation, s.pairingOnRelay, s.pairingNeedsPermission, s.pairingPermissionDenied, s.pairingNotified, s.pairedDevices, pairingBusy, pairingError, language]);
  if (el.dataset.signature === signature) return;
  const focused = el.contains(document.activeElement) ? (document.activeElement as HTMLElement).id : '';
  el.dataset.signature = signature;
  el.innerHTML = pairingPanelView(s, pairingBusy, pairingError, t);
  document.getElementById('pairing-open')?.addEventListener('click', () => void pairingAction('pair-iphone'));
  document.getElementById('pairing-cancel')?.addEventListener('click', () => void pairingAction('cancel-pairing'));
  if (focused) (document.getElementById(focused) ?? document.getElementById('pairing-cancel') ?? document.getElementById('pairing-open'))?.focus();
}
function renderPublicName() {
  const el = document.getElementById('public-name-panel'); if (!el || !status || !settings) return;
  const v = settings.values;
  const signature = JSON.stringify([status.publicName, status.receiverUrls, v.publicNameEnabled, v.publicNameTokenSet, v.publicName, v.publicNameProvider, publicNameBusy, publicNameMessage, language]);
  if (el.dataset.signature === signature) return;
  const focused = el.contains(document.activeElement) ? (document.activeElement as HTMLElement).id : '';
  el.dataset.signature = signature;
  el.innerHTML = publicNamePanel(status, settings, publicNameBusy, publicNameMessage, t, language);
  document.getElementById('public-name-renew')?.addEventListener('click', () => void publicNameAction('public-name-renew'));
  document.getElementById('public-name-remove')?.addEventListener('click', confirmRemovePublicName);
  if (focused) (document.getElementById(focused) ?? document.getElementById(fieldId('publicNameEnabled')))?.focus();
}
async function publicNameAction(name: 'public-name-renew' | 'public-name-remove') {
  if (publicNameBusy) return;
  const current = epoch;
  publicNameBusy = true; publicNameMessage = ''; renderPublicName();
  try {
    await api.action(name);
    const [nextStatus, nextSettings] = await Promise.all([fetchStatus(), name === 'public-name-remove' ? api.settings() : Promise.resolve(null)]);
    if (current !== epoch) return;
    status = nextStatus;
    if (nextSettings) {
      // The file is gone: unsaved edits of its fields go with it, others stay.
      for (const [key, item] of [...pending]) { if (key.startsWith('publicName')) { clearTimeout(item.timer); pending.delete(key); feedback.delete(key); } else nextSettings.values[key] = item.value; }
      settings = nextSettings; publicNameBusy = false; render(); document.getElementById(fieldId('publicNameEnabled'))?.focus();
    }
  } catch (error) { if (current === epoch) publicNameMessage = errorKey(error); }
  finally { if (current === epoch) { publicNameBusy = false; renderPublicName(); } }
}
function confirmRemovePublicName() {
  const dialog = document.createElement('dialog'); dialog.setAttribute('aria-labelledby', 'public-name-remove-title');
  dialog.innerHTML = `<h2 id="public-name-remove-title">${t('publicName.remove')}</h2><p>${t('publicName.removeHint')}</p><div class="dialog-actions"><button class="cancel">${t('ui.cancel')}</button><button class="confirm">${t('publicName.remove')}</button></div>`;
  document.body.append(dialog); dialog.showModal();
  const close = () => { dialog.close(); dialog.remove(); document.getElementById('public-name-remove')?.focus(); };
  dialog.querySelector('.cancel')!.addEventListener('click', close);
  dialog.addEventListener('cancel', event => { event.preventDefault(); close(); });
  dialog.querySelector('.confirm')!.addEventListener('click', () => { close(); void publicNameAction('public-name-remove'); });
}
function renderDiagnostics() {
  const el = document.getElementById('diagnostics'); if (!el || !status) return;
  const signature = JSON.stringify([status.video, status.receivers, status.bluetoothGuard, language]);
  if (el.dataset.signature === signature) return;
  el.dataset.signature = signature;
  const open = new Set([...el.querySelectorAll<HTMLDetailsElement>('details[data-details]')].filter(d => d.open).map(d => d.dataset.details));
  const focused = (document.activeElement as HTMLElement | null)?.dataset.report;
  el.innerHTML = diagnosticsView(status, t);
  el.querySelectorAll<HTMLDetailsElement>('details[data-details]').forEach(d => { d.open = open.has(d.dataset.details); });
  el.querySelectorAll<HTMLButtonElement>('[data-report]').forEach(button => {
    const feedback = button.parentElement!.querySelector<HTMLElement>('.report-feedback')!;
    if (reportCopied && reportCopied.client === button.dataset.report && reportCopied.until > Date.now()) feedback.textContent = t(reportCopied.key);
    button.addEventListener('click', () => void copyReport(button.dataset.report!));
  });
  if (focused) el.querySelector<HTMLButtonElement>(`[data-report="${CSS.escape(focused)}"]`)?.focus();
}
async function copyReport(client: string) {
  const report = status && receiverReport(status, client);
  if (!report) return;
  const copied = await copyText(report, navigator.clipboard, window.isSecureContext, text => {
    const area = document.createElement('textarea'); area.value = text; area.setAttribute('readonly', ''); area.style.position = 'fixed'; area.style.opacity = '0';
    document.body.append(area); area.select();
    try { return document.execCommand('copy'); } finally { area.remove(); }
  });
  if (!copied) {
    const { dialog } = networkDialog(t('diag.copy'), `<p>${t('diag.copyManual')}</p><textarea class="report-text" readonly rows="12">${esc(report)}</textarea><div class="dialog-actions"><button data-close>${t('ui.close')}</button></div>`);
    dialog.querySelector<HTMLTextAreaElement>('textarea')!.select();
    return;
  }
  reportCopied = { client, key: 'ui.copied', until: Date.now() + 2000 };
  document.querySelectorAll<HTMLButtonElement>('[data-report]').forEach(button => { if (button.dataset.report === client) button.parentElement!.querySelector<HTMLElement>('.report-feedback')!.textContent = t('ui.copied'); });
  setTimeout(() => {
    if ((reportCopied?.until ?? 0) > Date.now()) return;
    reportCopied = null;
    document.querySelectorAll<HTMLElement>('.report-feedback').forEach(feedback => { feedback.textContent = ''; });
  }, 2000);
}
function bindCopies(scope: ParentNode) {
  scope.querySelectorAll<HTMLButtonElement>('[data-copy]').forEach(button => {
    const previous = copied.get(button.dataset.copy!);
    if (previous && previous.until > Date.now()) button.querySelector<HTMLElement>('.copy-feedback')!.textContent = t(previous.key);
    button.addEventListener('click', async () => {
    const target = button.parentElement!.querySelector<HTMLElement>('.copy-text')!;
    const feedback = button.querySelector<HTMLElement>('.copy-feedback')!;
    try {
      if (!window.isSecureContext || !navigator.clipboard) throw new Error();
      await navigator.clipboard.writeText(button.dataset.copy!); feedback.textContent = t('ui.copied'); copied.set(button.dataset.copy!, { key: 'ui.copied', until: Date.now() + 2000 });
    } catch {
      if (target.dataset.secret !== undefined) {
        target.textContent = target.dataset.secret;
        const reveal = target.parentElement!.querySelector('[data-secret-toggle]'); reveal?.setAttribute('aria-pressed', 'true'); reveal?.setAttribute('aria-label', t('ui.hide'));
      }
      const range = document.createRange(); range.selectNodeContents(target); const selection = window.getSelection(); selection?.removeAllRanges(); selection?.addRange(range); target.focus(); feedback.textContent = t('ui.copyError'); copied.set(button.dataset.copy!, { key: 'ui.copyError', until: Date.now() + 2000 });
    }
    setTimeout(() => {
      if ((copied.get(button.dataset.copy!)?.until ?? 0) > Date.now()) return;
      copied.delete(button.dataset.copy!);
      document.querySelectorAll<HTMLButtonElement>('[data-copy]').forEach(current => { if (current.dataset.copy === button.dataset.copy) current.querySelector<HTMLElement>('.copy-feedback')!.textContent = ''; });
    }, 2000);
    });
  });
  scope.querySelectorAll<HTMLButtonElement>('[data-secret-toggle]').forEach(button => button.addEventListener('click', () => {
    const text = button.parentElement!.querySelector<HTMLElement>('[data-secret]')!;
    const reveal = button.getAttribute('aria-pressed') !== 'true';
    text.textContent = reveal ? text.dataset.secret! : '••••••••';
    button.setAttribute('aria-pressed', String(reveal)); button.setAttribute('aria-label', t(reveal ? 'ui.hide' : 'ui.reveal'));
    button.innerHTML = icon(reveal ? 'eye-off' : 'eye');
  }));
}
function renderCarlink() {
  if (!status || !carlinkView(status).visible) { syncPinDialog(); return; }
  const c = status.carlink;
  document.getElementById('context-action')!.insertAdjacentHTML('beforeend', carlinkConnectView(status, connectPin, connectBusy, carlinkError, t));
  const connectInput = document.getElementById('carlink-connect-pin') as HTMLInputElement;
  connectInput.addEventListener('input', () => { connectPin = connectInput.value; });
  document.getElementById('carlink-connect-form')!.addEventListener('submit', async event => {
    event.preventDefault();
    if (connectBusy) return;
    if (!canConnectCarlink(connectPin, c)) { carlinkError = 'error.carlinkPin'; updateDashboard(); return; }
    connectBusy = true; const current = epoch;
    const pin = connectPin; connectPin = ''; updateDashboard();
    try { await api.carlinkConnect(pin || undefined); if (current === epoch) { carlinkError = ''; void tick(); } }
    catch (error) { if (current === epoch) carlinkError = errorKey(error); }
    finally { if (current === epoch) { connectBusy = false; updateDashboard(); } }
  });
  document.getElementById('carlink-enter-pin')?.addEventListener('click', () => { dismissedPin = null; syncPinDialog(); });
  document.getElementById('carlink-forget')?.addEventListener('click', confirmForgetCarlink);
  syncPinDialog();
}
function syncPinDialog() {
  let dialog = document.querySelector<HTMLDialogElement>('#carlink-pin-dialog');
  const view = status && carlinkView(status);
  const deadline = status?.carlink?.pinDeadline ?? null;
  if (session !== 'ready' || switching || !view?.pinRequested || dismissedPin === deadline) {
    if (dialog) { dialog.close(); dialog.remove(); }
    return;
  }
  if (dialog && (dialog.dataset.deadline !== String(deadline) || dialog.dataset.language !== language)) {
    dialog.close(); dialog.remove(); dialog = null;
  }
  if (!dialog) {
    dialog = document.createElement('dialog'); dialog.id = 'carlink-pin-dialog';
    dialog.dataset.deadline = String(deadline); dialog.dataset.language = language;
    dialog.setAttribute('aria-labelledby', 'carlink-pin-title');
    dialog.innerHTML = `<form><h2 id="carlink-pin-title">${t('carlink.enterPin')}</h2><p>${t('carlink.pinHint')}</p><label for="carlink-pin">${t('carlink.pin')}</label><input id="carlink-pin" name="pin" type="text" inputmode="numeric" pattern="[0-9]{4,8}" minlength="4" maxlength="8" autocomplete="off" required aria-describedby="carlink-countdown carlink-pin-error"><p id="carlink-countdown" role="timer"></p><p id="carlink-pin-error" class="error" role="alert"></p><div class="dialog-actions"><button type="button" class="cancel">${t('ui.cancel')}</button><button type="submit" class="primary">${t('carlink.submitPin')}</button></div></form>`;
    document.body.append(dialog); dialog.showModal();
    const currentDialog = dialog;
    const close = () => { dismissedPin = deadline; currentDialog.close(); currentDialog.remove(); document.getElementById('carlink-enter-pin')?.focus(); };
    dialog.querySelector('.cancel')!.addEventListener('click', close);
    dialog.addEventListener('cancel', event => { event.preventDefault(); close(); });
    dialog.querySelector('form')!.addEventListener('submit', async event => {
      event.preventDefault();
      const input = currentDialog.querySelector<HTMLInputElement>('input')!;
      const errorEl = currentDialog.querySelector<HTMLElement>('#carlink-pin-error')!;
      if (!validCarlinkPin(input.value)) { errorEl.textContent = t('error.carlinkPin'); return; }
      const button = currentDialog.querySelector<HTMLButtonElement>('[type="submit"]')!; button.disabled = true;
      const current = epoch;
      try {
        await api.carlinkPin(input.value);
        input.value = '';
        if (current === epoch && status?.carlink?.pinDeadline === deadline) {
          status.carlink.pinRequested = false; status.carlink.pinDeadline = null;
          carlinkError = ''; close(); updateDashboard();
        }
      } catch (error) { errorEl.textContent = t(errorKey(error), error instanceof ApiError ? { seconds: error.retryAfter } : undefined); }
      finally { button.disabled = false; }
    });
  }
  dialog.querySelector('#carlink-countdown')!.textContent = t('carlink.countdown', { seconds: view.seconds });
}
function confirmForgetCarlink() {
  const dialog = document.createElement('dialog'); dialog.setAttribute('aria-labelledby', 'carlink-forget-title');
  dialog.innerHTML = `<h2 id="carlink-forget-title">${t('carlink.forget')}</h2><p>${t('carlink.forgetHint')}</p><p class="error" role="alert"></p><div class="dialog-actions"><button class="cancel">${t('ui.cancel')}</button><button class="confirm">${t('carlink.forget')}</button></div>`;
  document.body.append(dialog); dialog.showModal();
  const close = () => { dialog.close(); dialog.remove(); document.getElementById('carlink-forget')?.focus(); };
  dialog.querySelector('.cancel')!.addEventListener('click', close);
  dialog.addEventListener('cancel', event => { event.preventDefault(); close(); });
  dialog.querySelector('.confirm')!.addEventListener('click', async event => {
    const button = event.currentTarget as HTMLButtonElement; button.disabled = true;
    try { await api.action('forget-carlink'); carlinkError = ''; close(); void tick(); }
    catch (error) { carlinkError = errorKey(error); dialog.querySelector('p.error')!.textContent = t(carlinkError); }
    finally { button.disabled = false; }
  });
}
function topology(mode: Illustration): string {
  const shared = mode === 'shared-wifi';
  const phoneHotspot = mode === 'phone-hotspot' || mode === 'device-hotspot';
  const y = shared ? 90 : 48;
  const node = (name: string, x: number, top: number) => icon(name).replace('<svg ', `<svg x="${x - 12}" y="${top - 12}" `);
  const links = shared ? 'M155 28L45 78 M155 28V78 M155 28L265 78' : 'M57 48H143 M167 48H253';
  const labelOne = t(phoneHotspot ? 'ui.hotspot' : 'ui.wifiDirect');
  const labelTwo = t(phoneHotspot ? 'ui.hotspot' : 'ui.wifi');
  return `<figure class="topology"><svg viewBox="0 0 310 ${shared ? 136 : 94}" aria-hidden="true" focusable="false"><g fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"><path d="${links}" stroke-dasharray="4 4"/></g>${shared ? node('router', 155, 16) : ''}${node('smartphone', 45, y)}${node('router', 155, y)}${node('car', 265, y)}<g fill="currentColor" font-size="11" text-anchor="middle">${shared ? `<text x="211" y="23">${t('ui.wifi')}</text>` : `<text x="100" y="29">${labelOne}</text><text x="210" y="29">${labelTwo}</text>`}<text x="45" y="${y + 32}">${t('ui.iphone')}</text><text x="155" y="${y + 32}">${t('ui.relay')}</text><text x="265" y="${y + 32}">${t('ui.screen')}</text></g></svg><figcaption>${t(`ui.topology.${mode}`)}</figcaption></figure>`;
}
function fieldMarkup(field: Field): string {
  const id = fieldId(field.key); const value = settings!.values[field.key];
  const label = esc(t(field.labelKey)); const desc = `${id}-hint ${id}-result`;
  const invalid = feedback.has(field.key) && !feedback.get(field.key)!.startsWith('ui.');
  const attributes = `id="${id}" aria-describedby="${desc}" aria-invalid="${invalid}"`;
  let control = '';
  if (field.type === 'plugins') control = `<div ${attributes}>${pluginList(settings!, language, t, developer)}</div>`;
  else if (field.type === 'bool') control = `<label class="switch-row" for="${id}"><span>${label}</span><input ${attributes} type="checkbox" role="switch" ${value ? 'checked' : ''}><span class="switch-track" aria-hidden="true"></span></label>`;
  else if (field.type === 'choice') control = `<fieldset ${attributes}><legend>${label}</legend><div class="choices">${field.options?.map((option, i) => `<label class="choice"><div class="choice-label"><input type="radio" name="${id}" id="${id}-${i}" value="${esc(option.value)}" ${field.key === 'networkMode' ? networkOptionState(status?.platform, settings!.values, option.value) : ''} ${option.value === value ? 'checked' : ''}><span>${esc(t(option.labelKey))}</span></div>${(option.hintKey || field.key === 'output') ? `<p class="muted">${esc(t(option.hintKey || `hint.option.output.${option.value}`))}</p>` : ''}${option.illustration ? topology(option.illustration) : ''}</label>`).join('') ?? ''}</div></fieldset>`;
  else if (field.type === 'phone') control = `<label for="${id}">${label}</label><button ${attributes} class="picker-button" aria-haspopup="dialog">${icon('smartphone')}<span>${value === 'auto' ? t('ui.automatic') : esc(phoneNames.get(String(value)) ?? value)}</span>${icon('chevron-down')}</button>`;
  else control = `<label for="${id}">${label}</label><div class="input-row"><input ${attributes} type="${field.type === 'secret' ? 'password' : field.type}" value="${esc(value)}" ${field.min !== undefined ? `min="${field.min}"` : ''} ${field.max !== undefined ? `max="${field.max}"` : ''} ${field.type === 'number' ? 'step="1"' : ''} ${field.type === 'secret' ? 'autocomplete="new-password"' : ''} ${field.type === 'secret' && settings!.values[`${field.key}Set`] === true ? `placeholder="${esc(t(`${field.labelKey}Saved`))}"` : ''}>${field.type === 'secret' ? `<button type="button" class="icon-button reveal" aria-label="${t('ui.reveal')}" aria-pressed="false">${icon('eye')}</button>` : ''}</div>`;
  return `<div class="field" data-field="${esc(field.key)}">${control}${['output', 'transport'].includes(field.key) ? connectionConstraint(status?.platform, t) : ''}<p class="muted hint" id="${id}-hint">${field.hintKey ? esc(t(field.hintKey)) : ''}</p><p id="${id}-result" class="field-result ${invalid ? 'error' : ''}" role="status" aria-live="polite">${feedback.has(field.key) ? esc(t(feedback.get(field.key)!)) : ''}</p></div>`;
}
function bindField(field: Field) {
  const el = document.getElementById(fieldId(field.key))!;
  if (field.type === 'plugins') el.addEventListener('change', event => {
    const input = event.target as HTMLInputElement;
    queueValue(field, togglePlugin(settings!.values.pluginsEnabled, input.dataset.plugin!, input.checked));
  });
  else if (field.type === 'phone') el.addEventListener('click', () => void pickPhone(field));
  else if (field.type === 'choice') el.addEventListener('change', event => queueSave(field, (event.target as HTMLInputElement).value));
  else el.addEventListener(field.type === 'bool' ? 'change' : 'input', () => {
    const input = el as HTMLInputElement;
    queueSave(field, field.type === 'bool' ? input.checked : input.value);
  });
  if (field.type === 'secret') el.parentElement!.querySelector('button')!.addEventListener('click', event => {
    const input = el as HTMLInputElement; const reveal = input.type === 'password'; input.type = reveal ? 'text' : 'password';
    const button = event.currentTarget as HTMLButtonElement; button.setAttribute('aria-pressed', String(reveal)); button.setAttribute('aria-label', t(reveal ? 'ui.hide' : 'ui.reveal')); button.innerHTML = icon(reveal ? 'eye-off' : 'eye');
  });
}
function networkDialog(title: string, body: string) {
  const trigger = document.activeElement as HTMLElement | null;
  const triggerId = trigger?.id;
  const dialog = document.createElement('dialog'); dialog.setAttribute('aria-labelledby', 'network-dialog-title');
  dialog.innerHTML = `<h2 id="network-dialog-title">${esc(title)}</h2>${body}`;
  document.body.append(dialog); dialog.showModal();
  const close = () => {
    dialog.close(); dialog.remove();
    (trigger?.isConnected ? trigger : triggerId ? document.getElementById(triggerId) : null)?.focus();
    if (!document.activeElement || document.activeElement === document.body) document.getElementById('network-add')?.focus();
  };
  dialog.addEventListener('cancel', event => { event.preventDefault(); close(); });
  dialog.querySelectorAll('[data-close]').forEach(button => button.addEventListener('click', close));
  bindCopies(dialog);
  return { dialog, close };
}
function renderRestart() {
  const el = document.getElementById('restart-required'); if (!el || !status) return;
  const required = needsRestart || restartRequired(status.notices);
  el.hidden = !required;
  const signature = `${required}:${language}`;
  if (el.dataset.signature === signature) return;
  el.dataset.signature = signature; el.innerHTML = restartView(required, t);
  el.querySelector<HTMLButtonElement>('#restart-now')?.addEventListener('click', async event => {
    if (!window.confirm(t('home.restartHint'))) return;
    const button = event.currentTarget as HTMLButtonElement; button.disabled = true;
    try { await api.action('restart'); needsRestart = false; void tick(); }
    catch (error) { el.querySelector('#restart-error')!.textContent = t(errorKey(error)); }
    finally { button.disabled = false; }
  });
}
function editHotspot() {
  if (!status?.hotspot) return;
  const { dialog, close } = networkDialog(t('hotspot.change'), hotspotForm(status.hotspot.ssid, t));
  const name = dialog.querySelector<HTMLInputElement>('#hotspot-name')!;
  const password = dialog.querySelector<HTMLInputElement>('#hotspot-password')!;
  dialog.querySelector('#hotspot-reveal')!.addEventListener('click', event => {
    const reveal = password.type === 'password'; password.type = reveal ? 'text' : 'password';
    const button = event.currentTarget as HTMLButtonElement;
    button.setAttribute('aria-pressed', String(reveal)); button.setAttribute('aria-label', t(reveal ? 'ui.hide' : 'ui.reveal')); button.innerHTML = icon(reveal ? 'eye-off' : 'eye');
  });
  dialog.querySelector('form')!.addEventListener('submit', event => {
    event.preventDefault();
    const error = validateHotspot(name.value, password.value);
    dialog.querySelector('#hotspot-error')!.textContent = t(error);
    name.setAttribute('aria-invalid', String(error === 'error.hotspotSsid'));
    password.setAttribute('aria-invalid', String(error === 'error.hotspotPassword'));
    if (error) return;
    const ssid = name.value, secret = password.value;
    const active = status!.network.mode === 'device-hotspot';
    // Keep the form open underneath so Cancel returns to the entered values.
    const confirmation = networkDialog(t('hotspot.confirm'), `<p>${esc(t(active ? 'hotspot.warning' : 'hotspot.later'))}</p><p>${esc(ssid)}</p><p class="error" role="alert"></p><div class="dialog-actions"><button data-close>${t('ui.cancel')}</button><button data-confirm class="primary">${t('hotspot.apply')}</button></div>`);
    confirmation.dialog.querySelector<HTMLButtonElement>('[data-confirm]')!.addEventListener('click', async event => {
      const button = event.currentTarget as HTMLButtonElement; button.disabled = true;
      try {
        await api.hotspot(ssid, secret);
        status!.hotspot = { ...status!.hotspot!, ssid, password: secret, hasPassword: true };
        confirmation.close(); close();
        networkAnnouncement = t(active ? 'hotspot.reconnecting' : 'ui.saved'); renderNetworkPanel(); void tick();
      } catch (error) { confirmation.dialog.querySelector('.error')!.textContent = t(errorKey(error)); }
      finally { button.disabled = false; }
    });
  });
}
function renderNetworkPanel() {
  const el = document.getElementById('network-panel'); if (!el || !settings || !status || switching) return;
  const v = networkPanelView(status, settings);
  const signature = JSON.stringify([v, status.network.followingCarlink, status.hotspot, status.wifiControl, networks, networkError, networkAnnouncement, language]);
  if (el.dataset.signature === signature) return;
  const focused = el.contains(document.activeElement) ? (document.activeElement as HTMLElement).id : '';
  el.dataset.signature = signature;
  el.innerHTML = `<div class="field"><h3>${t('net.title')}</h3>${connectionConstraint(status?.platform, t)}${status.network.followingCarlink ? `<p>${t('network.followingCarlink')}</p>` : ''}<div class="network-current"><h3>${t(v.label)}</h3><p role="status" aria-live="polite">${esc(t(v.statusKey, v.parameters))}</p>${topology(v.mode as Illustration)}${!v.shared || status.wifiClient?.setupHotspot ? hotspotDetails(status, t) : ''}${!v.shared && status.platform === 'linux' ? `<button id="hotspot-edit" aria-haspopup="dialog">${t('hotspot.change')}</button>` : ''}${v.receiver && !v.shared ? `<p>${esc(t('net.carHint', { url: v.receiver }))}</p>` : ''}</div><p>${t(v.shared ? 'net.useHotspot' : 'net.useShared')}</p><button id="network-switch" ${networkOptionState(status?.platform, settings!.values, v.target)} aria-haspopup="dialog">${t(v.shared ? 'net.switchHotspot' : v.needsNetwork ? 'net.addSwitch' : 'net.switchShared')}</button><p id="network-mode-result" class="error" role="alert">${esc(t(networkError))}</p><p id="network-announcement" role="status" aria-live="polite"></p><h3>${t('net.saved')}</h3><p class="muted">${t('net.note')}</p><div id="saved-networks">${v.list.map((entry, index) => `<button class="phone-row" id="saved-network-${index}" data-network="${index}" aria-haspopup="dialog"><span>${esc(entry.ssid)}</span><span class="badges">${savedNetworkTags(entry, status!, networks).map(key => `<span class="tag">${t(key)}</span>`).join('')}</span></button>`).join('')}</div><button id="network-add" ${v.full ? 'disabled' : ''} aria-haspopup="dialog">${t('net.add')}</button>${v.full ? `<p>${t('net.limit')}</p>` : ''}</div>`;
  document.getElementById('hotspot-edit')?.addEventListener('click', editHotspot);
  document.getElementById('network-switch')!.addEventListener('click', () => requestNetworkSwitch(v.target));
  document.getElementById('network-add')!.addEventListener('click', () => editSharedNetwork(null));
  bindSharedNetworks(); bindCopies(el);
  requestAnimationFrame(() => { const announcement = document.getElementById('network-announcement'); if (announcement) announcement.textContent = networkAnnouncement; });
  if (focused) document.getElementById(focused)?.focus();
}
async function saveSharedNetworks(next: SavedNetwork[]) {
  await api.save({ sharedWifiNetworks: sharedNetworkUpdate(next) });
  settings!.values.sharedWifiNetworks = next.map(({ ssid, hasPassword, password }) => ({ ssid, hasPassword: password === undefined ? hasPassword : !!password }));
  renderNetworkPanel();
}
function bindSharedNetworks() {
  document.querySelectorAll<HTMLButtonElement>('[data-network]').forEach(button => button.addEventListener('click', () => {
    const entry = savedNetworks(settings!)[Number(button.dataset.network)];
    const { dialog, close } = networkDialog(entry.ssid, `<div class="network-actions">${canConnectNetwork(entry, status!) ? `<button data-connect>${t('net.connect')}</button>` : ''}<button data-edit>${t('net.change')}</button><button data-delete>${t('net.delete')}</button><button data-close>${t('ui.close')}</button></div>`);
    dialog.querySelector('[data-edit]')!.addEventListener('click', () => { close(); editSharedNetwork(entry); });
    dialog.querySelector('[data-connect]')?.addEventListener('click', () => {
      close();
      const consequences = switchConsequences(status!, settings!, 'shared-wifi'); consequences.parameters.wifi = entry.ssid; consequences.change = 'net.joinWifi';
      void beginNetworkSwitch('shared-wifi', consequences, () => api.wifi(entry.ssid));
    });
    dialog.querySelector('[data-delete]')!.addEventListener('click', () => {
      close();
      const current = savedNetworkTags(entry, status!, networks).includes('net.current');
      const confirmation = networkDialog(t('net.deleteTitle', { ssid: entry.ssid }), `${current ? `<p>${t('net.disconnect')}</p>` : ''}<p class="error" role="alert"></p><div class="dialog-actions"><button data-close>${t('ui.cancel')}</button><button data-confirm>${t('net.delete')}</button></div>`);
      confirmation.dialog.querySelector('[data-confirm]')!.addEventListener('click', async event => {
        const button = event.currentTarget as HTMLButtonElement; button.disabled = true;
        try { await saveSharedNetworks(deleteSharedNetwork(savedNetworks(settings!), entry.ssid)); confirmation.close(); void tick(); }
        catch (error) { confirmation.dialog.querySelector('.error')!.textContent = t(error instanceof ApiError ? error.errors.sharedWifiNetworks ?? error.key : errorKey(error)); }
        finally { button.disabled = false; }
      });
    });
  }));
}
function editSharedNetwork(entry: SavedNetwork | null, afterSave?: () => void) {
  if (!settings || !status || (!entry && savedNetworks(settings).length >= 10)) return;
  const connect = !entry && addSavesAndConnects(status);
  const { dialog, close } = networkDialog(t(entry ? 'net.change' : 'net.add'), `<form><label for="network-name">${t('net.wifiName')}</label><input id="network-name" value="${esc(entry?.ssid ?? '')}" ${entry ? 'readonly' : ''} required autocomplete="off">${!entry && status.platform === 'linux' ? `<p>${t('wifi.only24')}</p>` : ''}${!entry && status.wifiControl ? `<section class="nearby"><div class="nearby-heading"><h3>${t('net.scan')}</h3><button type="button" id="network-refresh" class="icon-button" aria-label="${t('net.refresh')}" title="${t('net.refresh')}">${icon('refresh-cw')}</button></div><p id="network-scan-status" role="status"></p><p class="muted" ${status.wifiScanStale || status.hotspot?.active || status.wifiClient?.setupHotspot ? '' : 'hidden'}>${t('net.stale')}</p><div id="network-scan"></div></section>` : ''}<div id="network-secret"><label for="network-password">${t('net.password')}</label><div class="input-row"><input id="network-password" type="password" autocomplete="new-password" placeholder="${entry?.hasPassword ? t('networks.keep') : ''}"><button type="button" id="network-reveal" class="icon-button" aria-label="${t('ui.reveal')}" aria-pressed="false">${icon('eye')}</button></div></div><label class="check-row"><input id="network-open" type="checkbox" ${entry && !entry.hasPassword ? 'checked' : ''}>${t('net.open')}</label><p id="network-error" class="error" role="alert"></p>${connect ? `<p>${t('net.saveConnect')}</p>` : ''}<div class="dialog-actions"><button type="button" data-close>${t('ui.cancel')}</button><button type="submit" class="primary">${t('net.save')}</button></div></form>`);
  const name = dialog.querySelector<HTMLInputElement>('#network-name')!;
  const password = dialog.querySelector<HTMLInputElement>('#network-password')!;
  const open = dialog.querySelector<HTMLInputElement>('#network-open')!;
  const syncSecret = () => { dialog.querySelector<HTMLElement>('#network-secret')!.hidden = open.checked; };
  open.addEventListener('change', syncSecret); syncSecret();
  dialog.querySelector('#network-reveal')!.addEventListener('click', event => {
    const reveal = password.type === 'password'; password.type = reveal ? 'text' : 'password';
    const button = event.currentTarget as HTMLButtonElement; button.setAttribute('aria-pressed', String(reveal)); button.setAttribute('aria-label', t(reveal ? 'ui.hide' : 'ui.reveal')); button.innerHTML = icon(reveal ? 'eye-off' : 'eye');
  });
  const scan = async () => {
    const button = dialog.querySelector<HTMLButtonElement>('#network-refresh'); if (!button) return;
    button.disabled = true;
    const feedback = dialog.querySelector('#network-scan-status')!; feedback.textContent = t('ui.loading');
    try {
      const found = sortNetworks(await api.networks()); if (!dialog.isConnected) return;
      networks = found; renderNetworkPanel(); feedback.textContent = found.length ? '' : t('wifi.empty');
      const list = dialog.querySelector('#network-scan')!;
      list.innerHTML = nearbyNetworksView(found, t);
      list.querySelectorAll<HTMLButtonElement>('button').forEach(button => button.addEventListener('click', () => {
        const network = found[Number(button.dataset.nearby)]; name.value = network.ssid; password.value = ''; open.checked = !network.secure; syncSecret();
        (network.secure ? password : dialog.querySelector<HTMLButtonElement>('[type="submit"]'))?.focus();
      }));
    } catch (error) { feedback.textContent = t(errorKey(error)); }
    finally { button.disabled = false; }
  };
  dialog.querySelector('#network-refresh')?.addEventListener('click', () => void scan());
  if (!entry && status.wifiControl) void scan();
  dialog.querySelector('form')!.addEventListener('submit', async event => {
    event.preventDefault();
    const button = dialog.querySelector<HTMLButtonElement>('[type="submit"]')!;
    try {
      const credentials = wifiJoinCredentials(null, true, name.value, open.checked ? '' : password.value);
      if (!credentials) throw new Error('error.networks');
      const next = putSharedNetwork(savedNetworks(settings!), entry?.ssid ?? null, credentials.ssid, sharedNetworkPassword(entry, open.checked, password.value));
      button.disabled = true;
      if (connect) {
        const consequences = switchConsequences(status!, settings!, 'shared-wifi'); consequences.parameters.wifi = name.value;
        close(); await beginNetworkSwitch('shared-wifi', consequences, () => api.wifi(credentials.ssid, credentials.password));
      } else { await saveSharedNetworks(next); close(); afterSave?.(); }
    } catch (error) { dialog.querySelector('#network-error')!.textContent = t(error instanceof ApiError ? error.errors.sharedWifiNetworks ?? error.key : error instanceof NetworkError ? 'error.network' : 'error.networks'); }
    finally { button.disabled = false; }
  });
}
function requestNetworkSwitch(target: string) {
  if (!status || !settings || target === status.network.mode) return;
  if (wifiDirectDisabled(status.platform, settings.values, target)) { networkError = constraintKey; renderNetworkPanel(); return; }
  if (target === 'shared-wifi' && !savedNetworks(settings).length) { editSharedNetwork(null, () => requestNetworkSwitch(target)); return; }
  const c = switchConsequences(status, settings, target);
  const { dialog, close } = networkDialog(t(c.title), `<p>${esc(t(c.change, c.parameters))}</p><p>${t('net.iphone')}</p><p>${esc(t(c.car, c.parameters))}</p>${!c.shared && status.hotspot ? hotspotDetails(status, t) : ''}<p>${esc(t(c.comeback, c.parameters))}</p><div class="dialog-actions"><button data-close>${t('ui.cancel')}</button><button class="primary" data-confirm>${t(c.action)}</button></div>`);
  dialog.querySelector('[data-confirm]')!.addEventListener('click', () => { close(); void beginNetworkSwitch(target, c, () => api.switchNetwork(target)); });
}
async function beginNetworkSwitch(target: string, consequences: ReturnType<typeof switchConsequences>, mutation: () => Promise<unknown>) {
  switching = { target, consequences, started: Date.now() }; networkError = ''; networkAnnouncement = ''; render(); document.querySelector<HTMLElement>('#network-switching h1')?.focus();
  try { await mutation(); }
  catch (error) {
    // The radio may switch before its response reaches the browser. Poll; never resend.
    if (!(error instanceof NetworkError)) {
      switching = null; networkError = error instanceof ApiError ? error.errors.networkMode ?? error.key : errorKey(error); showSettings(); document.getElementById('network-switch')?.focus();
    }
  }
}
function renderSwitching() {
  const c = switching!.consequences;
  document.getElementById('connection')!.hidden = true;
  document.getElementById('main')!.innerHTML = `<section class="card" id="network-switching"><h1 tabindex="-1" role="status" aria-live="polite">${t('net.switching')}</h1><p>${esc(t(c.change, c.parameters))}</p><p>${t('net.iphone')}</p><p>${esc(t(c.car, c.parameters))}</p>${!c.shared && status ? hotspotDetails(status, t) : ''}<p>${esc(t(c.comeback, c.parameters))}</p></section>`;
  bindCopies(document.getElementById('network-switching')!);
}

function updateVisibility() {
  for (const field of fields()) {
    const el = document.getElementById(fieldId(field.key))?.closest<HTMLElement>('.field');
    if (el) el.hidden = !isVisible(field, settings!.values);
  }
  document.querySelectorAll<HTMLElement>('.settings-section').forEach(section => { section.hidden = !section.querySelector('#network-panel') && ![...section.querySelectorAll<HTMLElement>('.field')].some(field => !field.hidden); });
  renderPublicName();
}
function queueSave(field: Field, raw: string | boolean) {
  const value = coerceValue(field, raw);
  if (field.key === 'output' && needsHotspotConfirmation(status?.platform, settings!.values, value)) {
    if (!window.confirm(t('constraint.carlinkWifiDirectConfirm'))) { render(); return; }
    // The Android API applies the output and safe network mode together.
    settings!.values.networkMode = 'phone-hotspot';
  }
  if (field.key === 'transport' && value === 'wireless' && wifiDirectDisabled(status?.platform, settings!.values, settings!.values.networkMode)) settings!.values.networkMode = 'phone-hotspot';
  if (field.key === 'networkMode' && wifiDirectDisabled(status?.platform, settings!.values, value)) return;
  queueValue(field, value);
  if (field.key === 'output') render();
}
function queueValue(field: Field, value: Value) {
  settings!.values[field.key] = value;
  const item = pending.get(field.key) ?? { value, revision: 0, busy: false };
  clearTimeout(item.timer); item.value = value; item.revision++;
  pending.set(field.key, item); result(field.key, 'ui.saving');
  item.timer = setTimeout(() => void save(field.key), 450);
  updateVisibility();
}
async function save(key: string) {
  const item = pending.get(key); if (!item || item.busy || session !== 'ready' || handoff) return;
  if (switching) { item.timer = setTimeout(() => void save(key), 2000); return; }
  item.busy = true; const revision = item.revision; const current = epoch;
  try {
    const response = await api.save({ [key]: key === 'sharedWifiNetworks' ? sharedNetworkUpdate(item.value as SavedNetwork[]) : item.value });
    if (current !== epoch) return;
    if (revision === item.revision) { pending.delete(key); needsRestart ||= restartRequired([], response.applied); renderRestart(); result(key, response.applied === 'now' ? 'ui.saved' : 'ui.restartRequired'); }
    // A saved token is never shown again; the field then reads "Token saved".
    if (key === 'publicNameToken' && item.value !== '' && settings) settings.values.publicNameTokenSet = true;
  } catch (error) {
    if (current !== epoch) return;
    if (revision === item.revision) {
      if (error instanceof NetworkError) result(key, 'ui.unsaved');
      else { pending.delete(key); result(key, error instanceof ApiError ? error.errors[key] ?? error.key : 'error.request'); }
    }
  } finally {
    item.busy = false;
    if (pending.get(key) === item && current === epoch) item.timer = setTimeout(() => void save(key), revision === item.revision ? 2000 : 450);
  }
}
async function pickPhone(field: Field) {
  const trigger = document.getElementById(fieldId(field.key))!;
  const dialog = document.createElement('dialog'); dialog.setAttribute('aria-labelledby', 'phone-title');
  dialog.innerHTML = `<div class="sheet-header"><h2 id="phone-title">${t('ui.choosePhone')}</h2><button class="icon-button close" aria-label="${t('ui.close')}">${icon('x')}</button></div><div id="phone-list" role="status">${t('ui.loading')}</div>`;
  document.body.append(dialog); dialog.showModal();
  const close = () => { dialog.close(); dialog.remove(); trigger.focus(); };
  dialog.querySelector('.close')!.addEventListener('click', close);
  dialog.addEventListener('cancel', event => { event.preventDefault(); close(); });
  const current = epoch;
  const list = dialog.querySelector<HTMLElement>('#phone-list')!;
  const populate = (phones: Phone[]) => {
    phones.forEach(phone => phoneNames.set(phone.id, phone.name));
    const rows: Phone[] = [{ id: 'auto', name: t('ui.automatic'), kind: 'phone', connected: false, likelyIphone: false, selected: settings!.values[field.key] === 'auto' }, ...phones];
    list.removeAttribute('role');
    list.innerHTML = rows.map((phone, i) => `<button class="phone-row" data-index="${i}" aria-pressed="${settings!.values[field.key] === phone.id}">${icon(phone.id === 'auto' ? 'router' : phone.kind === 'car' ? 'car' : phone.kind === 'phone' ? 'smartphone' : 'bluetooth')}<span><strong>${esc(phone.name)}</strong><span class="badges">${phone.id !== 'auto' ? `<span>${esc(t(`ui.${phone.kind}`))}</span>` : ''}${phone.connected ? `<span class="tag">${t('ui.connected')}</span>` : ''}${phone.likelyIphone ? `<span class="tag">${t('ui.likelyIphone')}</span>` : ''}${phone.selected ? `<span>${t('ui.selected')}</span>` : ''}</span></span></button>`).join('') + (!phones.length ? `<p>${t('ui.noPhones')}</p>` : '');
    list.querySelectorAll<HTMLButtonElement>('button').forEach(button => button.addEventListener('click', () => {
      const phone = rows[Number(button.dataset.index)]; queueSave(field, phone.id);
      trigger.querySelector('span')!.textContent = phone.name; close();
    }));
  };
  try { const phones = await api.phones(); if (current === epoch && dialog.isConnected) populate(phones); else close(); }
  catch (error) { list.textContent = t(errorKey(error)); }
}
function bindPassword() {
  document.getElementById('password-form')!.addEventListener('submit', async event => {
    event.preventDefault();
    const form = event.target as HTMLFormElement;
    const value = (name: string) => (form.elements.namedItem(name) as HTMLInputElement).value;
    const result = document.getElementById('password-result')!;
    const invalid = validatePassword(value('new'), value('confirm'));
    if (invalid) { result.textContent = t(invalid); return; }
    const button = form.querySelector('button')!; button.disabled = true;
    try {
      await api.password(value('current'), value('new'));
      const next = await api.session(); defaultPassword = next.defaultPassword;
      document.querySelector('#menu-password small')?.remove();
      form.reset(); result.textContent = t('ui.passwordChanged');
    } catch (error) { result.textContent = t(errorKey(error)); }
    finally { button.disabled = false; }
  });
}
async function forgetAllWifi() {
  const trigger = document.activeElement as HTMLElement;
  if (status?.platform === 'android') {
    const dialog = document.createElement('dialog'); dialog.setAttribute('aria-labelledby', 'forget-wifi-title');
    dialog.innerHTML = `<h2 id="forget-wifi-title">${t('net.forgetAll')}</h2><p>${t('home.forgetSavedWifi')}</p><p class="error" role="alert"></p><div class="dialog-actions"><button class="cancel">${t('ui.cancel')}</button><button class="confirm">${t('net.forgetAll')}</button></div>`;
    document.body.append(dialog); dialog.showModal();
    const close = () => { dialog.close(); dialog.remove(); trigger?.focus(); };
    dialog.querySelector('.cancel')!.addEventListener('click', close);
    dialog.addEventListener('cancel', event => { event.preventDefault(); close(); });
    dialog.querySelector('.confirm')!.addEventListener('click', async event => {
      const button = event.currentTarget as HTMLButtonElement; button.disabled = true;
      try { await api.save({ sharedWifiNetworks: [] }); settings!.values.sharedWifiNetworks = []; close(); render(); document.getElementById('forget-all-wifi')?.focus(); }
      catch (error) { dialog.querySelector('.error')!.textContent = t(errorKey(error)); }
      finally { button.disabled = false; }
    });
    return;
  }
  const dialog = document.createElement('dialog'); dialog.setAttribute('aria-labelledby', 'wifi-confirm-title');
  dialog.innerHTML = `<h2 id="wifi-confirm-title">${t('net.forgetAll')}</h2><p>${esc(t(wifiGuidance('forget')))}</p><button id="wifi-cancel">${t('ui.cancel')}</button> <button id="wifi-confirm" class="primary">${t('net.forgetAll')}</button>`;
  document.body.append(dialog); dialog.showModal();
  const close = () => { dialog.close(); dialog.remove(); trigger?.focus(); };
  dialog.querySelector('#wifi-cancel')!.addEventListener('click', close);
  dialog.addEventListener('cancel', event => { event.preventDefault(); close(); });
  dialog.querySelector('#wifi-confirm')!.addEventListener('click', async () => {
    handoff = { action: 'forget', ssid: '' }; epoch++;
    for (const item of pending.values()) clearTimeout(item.timer);
    pending.clear(); close(); render();
    try { await api.action('forget-wifi'); }
    catch (error) {
      // A lost response is expected when the radio switches networks. Never retry the mutation.
      if (!(error instanceof NetworkError)) {
        handoff!.error = errorKey(error); render();
      }
    }
  });
}
function renderHandoff() {
  document.getElementById('connection')!.hidden = true;
  document.getElementById('main')!.innerHTML = `<section class="card auth"><h1>${t('wifi.reconnect')}</h1><p role="status">${esc(t(wifiGuidance(handoff!.action), { ssid: handoff!.ssid }))}</p>${handoff!.error ? `<p class="error" role="alert">${esc(t(handoff!.error!))}</p>` : ''}<p>${t('wifi.reloadHint')}</p></section>`;
  document.getElementById('logout')?.setAttribute('disabled', '');
}
async function tick() {
  if ((document.hidden && !switching) || polling || handoff) return;
  polling = true;
  try {
    if (!session || session !== 'ready') {
      const next = await api.session();
      if (next.state !== session || next.defaultPassword !== defaultPassword) { defaultPassword = next.defaultPassword; session = sessionRoute(next); render(); }
      if (session === 'ready') await loadReady();
    } else if (!status || !settings) await loadReady();
    else {
      const current = epoch; const next = await fetchStatus();
      if (current === epoch && session === 'ready') { status = next; renderRestart();
        if (switching && next.network.mode === switching.target && Date.now() - switching.started >= 2000) {
          const joinError = next.notices.find(n => n.startsWith('wifi.joinFailed.'));
          networkError = joinError ?? '';
          networkAnnouncement = joinError ? '' : t('net.switched', { mode: t(next.network.mode === 'shared-wifi' ? 'net.shared' : 'net.hotspot') });
          settings = await api.settings();
          for (const [key, item] of pending) settings.values[key] = item.value;
          switching = null; render(); document.getElementById('network-switch')?.focus();
        } else if (!switching) { updateDashboard(); renderNetworkPanel(); renderPairingPanel(); renderDiagnostics(); renderPublicName(); } }
    }
  } catch (error) { if (!(error instanceof NetworkError)) { pageError = errorKey(error); if (!status) render(); } }
  finally { polling = false; }
}
document.addEventListener('visibilitychange', () => { if (!document.hidden) void tick(); });
document.addEventListener('keydown', event => {
  if (!['ArrowUp', 'ArrowDown', 'ArrowLeft', 'ArrowRight'].includes(event.key) || event.altKey || event.ctrlKey || event.metaKey) return;
  const target = event.target as HTMLElement;
  if (target.matches('select, textarea') || target.isContentEditable) return;
  if (target.matches('input') && ['ArrowLeft', 'ArrowRight'].includes(event.key)) return;
  const scope = document.querySelector('dialog[open]') ?? document;
  const items = [...scope.querySelectorAll<HTMLElement>('button:not(:disabled), a[href], summary, input, select, [tabindex="0"]')].filter(el => el.getClientRects().length > 0);
  const index = items.indexOf(target); const direction = ['ArrowUp', 'ArrowLeft'].includes(event.key) ? -1 : 1;
  const next = items[Math.max(0, Math.min(items.length - 1, index + direction))];
  if (next) { event.preventDefault(); next.focus(); }
});
setInterval(syncPinDialog, 1000);
createCountdown(() => status?.pairingDeadline, seconds => {
  const time = formatRemaining(seconds);
  const home = document.getElementById('pair-countdown'); if (home) home.textContent = seconds ? t('home.pairCountdown', { time }) : '';
  const panel = document.getElementById('pairing-remaining'); if (panel) panel.textContent = t('pairing.open', { time });
}, () => { updateDashboard(); renderPairingPanel(); void tick(); }).start();
render(); void tick(); setInterval(() => void tick(), 2000);
