// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-15

import { constraintKey, connectionConstraint, networkOptionState, wifiDirectDisabled, needsHotspotConfirmation } from './carlink-connection';
import { needsUsbConfirmation, usbTransportHint } from './usb-connection';
import { pluginList, togglePlugin } from './plugins';
import { validateHotspot, hotspotForm, stationBandHint, devicePresetLabel, restartRequired, restartView } from './hotspot-settings';
import './style.css';
import { patchMarkup, on, enhanceSheets, confirmSheet } from './dom';
import { mountStage, timer } from './stage';
import { presentPath } from './path/present-path';
import { stageCommand } from './path/actions';
import type { StageAction } from './path/present-stage';
import { autoFrameRateSummary, fieldRoute, pageFields, pages, developerPages, routeTitle, parentRoute, normalizeRoute, restartsPicture, menuSummary, pageAvailable, developerIcons } from './settings-tree';
import { mountPictureField } from './picture-sizes';
import { COUNTRY_CODES } from './wifi-countries';
import { mountFeaturePage } from './feature-pages';
import { connectionsView, editConnections } from './connections';
import type { NetworkRolesSetting } from './schema';
import { mountTrace, traceFilters, traceVisible } from './carlink-trace';
import { carlinkConnectView, statusView, addressesView, devicesView, hotspotDetails, nearbyNetworksView } from './views';
import { homeView, settingsViews, developerPreference, networkPanelView, switchConsequences, savedNetworks, savedNetworkTags, addSavesAndConnects, canConnectNetwork } from './logic';
import { putSharedNetwork, deleteSharedNetwork, sharedNetworkUpdate, sharedNetworkPassword } from './shared-networks';
import type { SavedNetwork } from './schema';
import { api, ApiError, NetworkError, events, type HotspotBand, type SessionState, type Status, type Phone, type WifiNetwork } from './api';
import { isVisible, optionLabel, coerceValue, validatePassword, type Field, type Settings, type Value, type Illustration } from './schema';
import { defaultLanguage, translate, type Language } from './i18n';
import { sessionRoute, wifiGuidance, sortNetworks, wifiJoinCredentials, carlinkView, carlinkPinError, canConnectCarlink, validCarlinkPin } from './logic';
import { icon } from './icons';
import { anchorPairing, createCountdown, formatRemaining, pairingPanelView } from './pairing';
import { VideoStatsSnapshot, videoStatsInterval, videoStatsView } from './video-stats';
import { copyText, diagnosticsView, receiverReport } from './diagnostics';
import { PublicNameDrafts, publicNamePanel, publicNameSubmission, publicNameGuidance, publicNameNameHint, importPublicNameFile, type PublicNameMessage } from './public-name';

let disposeTrace: (() => void) | null = null;
const root = document.querySelector<HTMLDivElement>('#app')!;
let languagePreference = (() => { try { const value = localStorage.getItem('openxplay.language'); return value === 'en' || value === 'zh-CN' ? value : 'auto'; } catch { return 'auto'; } })();
let language: Language = languagePreference === 'auto' ? defaultLanguage(navigator.language) : languagePreference as Language;
const t = (key: string, parameters?: Record<string, string | number>) => translate(language, key, parameters);
import { esc } from './html';
let session: SessionState | null = null;
let defaultPassword = false;
let route = normalizeRoute(location.hash);
let currentView = route === 'home' ? 'home' : 'settings';
let stage: ReturnType<typeof mountStage> | null = null;
let visibleTimer: ReturnType<typeof setInterval> | undefined;
const drafts = new Map<string, string>();
on(window, 'hashchange', () => { videoStats.reset(); route = normalizeRoute(location.hash); currentView = route === 'home' ? 'home' : 'settings'; render(); });
function showSettings(target?: string) {
  const next = target?.startsWith('settings') ? target : target === 'current-password' ? 'settings/access/admin' : target?.startsWith('field-') ? fieldRoute(decodeURIComponent(target.slice(6))) : 'settings';
  location.hash = next;
  if (route === next) render();
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
const videoStats = new VideoStatsSnapshot();
let carlinkError = '';
let connectPin = '';
let connectBusy = false;
let dismissedPin: number | null = null;
let pairingBusy = false;
let pairingError = '';
let publicNameBusy = false;
let publicNameMessage: PublicNameMessage = '';
let publicNameDrafts = new PublicNameDrafts();
let publicNameWrite: Promise<unknown> | null = null;
let reportCopied: { client: string; key: string; until: number } | null = null;
const copied = new Map<string, { key: string; until: number }>();
const phoneNames = new Map<string, string>();
const feedback = new Map<string, string>();
const pending = new Map<string, { value: Value; revision: number; timer?: ReturnType<typeof setTimeout>; busy: boolean }>();
const fields = () => settings?.schema.flatMap(section => section.fields) ?? [];
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
on(events, 'offline', () => { offline = true; banner(); });
on(events, 'online', () => { offline = false; banner(); });
function resetSession(next: SessionState) {
  epoch++;
  for (const item of pending.values()) clearTimeout(item.timer);
  videoStats.reset();
  needsRestart = false; drafts.clear(); pending.clear(); feedback.clear(); networks = []; switching = null; networkError = ''; networkAnnouncement = ''; status = null; settings = null; carlinkError = ''; connectPin = ''; connectBusy = false; dismissedPin = null; pairingBusy = false; pairingError = ''; reportCopied = null; publicNameBusy = false; publicNameMessage = '';
  publicNameDrafts = new PublicNameDrafts(); publicNameWrite = null;
  document.querySelectorAll('dialog').forEach(dialog => { dialog.close(); dialog.remove(); });
  session = next; actionBusy = false; render();
}
on(events, 'unauthorized', () => { if (session === 'ready' && !handoff) resetSession('login'); });
// Anchor deadlines on this browser's clock; statistics are fetched only when displayed.
async function fetchStatus(): Promise<Status> { return anchorPairing(await api.status(route === 'settings/developer/diagnostics' || route === 'settings/connection/network/interfaces' || currentView === 'home' && videoStatsInterval(settings, status?.platform) > 0), Date.now() / 1000, status?.pairingDeadline ?? null); }
function errorKey(error: unknown): string { return error instanceof ApiError ? error.key : error instanceof NetworkError ? 'error.network' : 'error.request'; }
function shell() {
  document.documentElement.lang = language;
  document.body.dataset.page = currentView;
  const title = t(routeTitle(route));
  patchMarkup(root, `<header class="top-bar">${session === 'ready' ? currentView === 'home' ? `<a class="icon-button" href="#settings" aria-label="${t('ui.settings')}">${icon('settings')}</a>` : `<a class="icon-button back" href="#${parentRoute(route)}" aria-label="${t('ui.back')}">${icon('chevron-left')}</a><span id="bar-title" aria-hidden="true">${esc(title)}</span>` : `<span>OpenXPlay Relay</span>`}</header><div id="connection" role="status" class="banner" ${offline ? '' : 'hidden'}>${t('ui.reconnecting')}</div><div id="restart-required" class="card" hidden></div><main id="main"></main>`);
}
function render() {
  stage?.destroy(); stage = null; clearInterval(visibleTimer); visibleTimer = undefined;
  disposeTrace?.(); disposeTrace = null;
  shell();
  if (switching) renderSwitching();
  else if (handoff) renderHandoff();
  else if (session === 'login') renderAuth();
  else if (session === 'ready' && settings && status) renderReady();
  else patchMarkup(document.getElementById('main')!, `<p role="status">${esc(t(pageError || 'ui.loading'))}</p>`);
}
function renderAuth() {
  patchMarkup(document.getElementById('main')!, `<section class="card auth"><div class="eyebrow">${t('ui.admin')}</div><h1>${t('ui.login')}</h1><p class="muted">${t('ui.loginHint')}</p>${defaultPassword ? `<p>${t('ui.firstTime')}</p>` : ''}<form id="auth-form"><label for="password">${t('ui.password')}</label><input id="password" name="password" type="password" autocomplete="current-password" required aria-describedby="auth-error"><p id="auth-error" class="error" role="alert"></p><button class="primary" type="submit">${t('ui.signIn')}</button></form></section>`);
  on(document.getElementById('auth-form')!, 'submit', async event => {
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
function bindConnections() {
  on(document.getElementById('connections-edit'), 'click', event => {
    editConnections(status!, settings!.values.networkRoles as NetworkRolesSetting | undefined, t, loadReady, event.currentTarget as HTMLElement);
  });
}
function renderConnections() {
  const current = document.querySelector<HTMLElement>('[data-section="connections"]');
  if (!current || !status) return;
  const open = current.querySelector('details')?.open;
  const template = document.createElement('div'); patchMarkup(template, connectionsView(status, t)); const next = template.firstElementChild; if (next) { for (const attr of [...next.attributes]) current.setAttribute(attr.name, attr.value); patchMarkup(current, next.innerHTML); }
  const details = document.querySelector<HTMLDetailsElement>('.interface-inventory');
  if (details) details.open = !!open;
  bindConnections();
}
function menuRow(target: string, title: string, summary = '', name = 'settings'): string {
  const error = fields().filter(f => fieldRoute(f.key) === target || fieldRoute(f.key).startsWith(target + '/')).some(f => { const message = feedback.get(f.key); return !!message && !message.startsWith('ui.'); });
  return `<a class="inset-row menu-row" href="#${target}"><span class="icon-tile hue-${name}">${icon(name)}</span><span class="row-copy"><span>${esc(t(title))}</span>${summary ? `<small class="muted">${esc(summary)}</small>` : ''}</span>${error ? `<span class="error" aria-label="${t('error.invalid')}">!</span>` : ''}<span aria-hidden="true">›</span></a>`;
}
function pageSummary(target: string): string { return menuSummary(settings!, status!, target, language, t); }
function editorField(field: Field): Field {
  if (field.key !== 'wifiCountry') return field;
  const names = new Intl.DisplayNames([language], { type: 'region' });
  return { ...field, type: 'choice', options: [{ value: 'system', labelKey: 'roles.country.system' }, ...COUNTRY_CODES.map(value => ({ value, labelKey: '', labels: { en: names.of(value) ?? value, 'zh-CN': names.of(value) ?? value } }))] };
}
function renderReady() {
  if (!pageAvailable(settings!, status!, route) && route !== 'home') { location.hash = parentRoute(route); return; }
  renderRestart();
  const main = document.getElementById('main')!;
  if (currentView !== 'home') videoStats.reset();
  if (currentView === 'home') {
    patchMarkup(main, '<div id="dashboard"></div>');
    stage = mountStage(document.getElementById('dashboard')!, { t, action: action => void dispatchStage(action), copy: value => void copyStage(value),
      details: () => `${menuRow('settings/car', 'menu.car', pageSummary('settings/car'), 'car')}<div class="inset-group">${fields().filter(f => f.key === 'autoStart').map(f => fieldMarkup({ ...f, labelKey: 'stage.automatic', hintKey: undefined })).join('')}</div>${addressesView(status!, true, t)}${devicesView(status!, true, t)}<div id="pairing-panel" data-preserve></div>${videoStatsView(videoStats.read(status!, videoStatsInterval(settings, status!.platform), performance.now()), t)}${status!.notices.length ? `<div class="inset-group">${status!.notices.map(n => `<p>${esc(t(n))}</p>`).join('')}</div>` : ''}`,
      bindDetails: scope => { bindCopies(scope); const auto = fields().find(f => f.key === 'autoStart'); if (auto) bindField(auto); renderPairingPanel(); syncVisibleTimer(); },
      notice: notice => { if (notice === 'DISPLAY_CHANGED') void dispatchStage({ type: 'Step', action: 'RETRY', step: 'CARPLAY_START', label: null }); else showSettings(notice === 'CHOOSE_CAR' ? 'settings/car' : 'settings'); },
    });
    updateDashboard(); return;
  }
  patchMarkup(main, `<div class="settings-page"><h1 class="large-title">${esc(t(routeTitle(route)))}</h1><div id="settings"></div></div>`);
  const container = document.getElementById('settings')!;
  if (route === 'settings') {
    patchMarkup(container, [pages.slice(0, 4), pages.slice(4, 6), pages.slice(6, 9), pages.slice(9)].map(group => `<div class="inset-group menu-group">${group.filter(([page]) => pageAvailable(settings!, status!, 'settings/' + page)).map(([page, title, name]) => menuRow('settings/' + page, title, pageSummary('settings/' + page), name)).join('')}</div>`).join(''));
  } else if (route === 'settings/developer') {
    patchMarkup(container, `<div class="inset-group">${developerPages.filter(page => pageAvailable(settings!, status!, 'settings/developer/' + page)).map(page => menuRow('settings/developer/' + page, 'menu.developer.' + page, pageSummary('settings/developer/' + page), developerIcons[page])).join('')}</div>`);
  } else if (['settings/logs', 'settings/config-files', 'settings/about'].includes(route)) {
    mountFeaturePage(container, route, { t, platform: status!.platform, adminFeatures: status!.adminFeatures, version: status!.version, language: languagePreference, setLanguage: preference => { languagePreference = preference; language = preference === 'auto' ? defaultLanguage(navigator.language) : preference as Language; try { localStorage.setItem('openxplay.language', preference); } catch {} render(); }, error: errorKey, reload: async () => { settings = await api.settings(); } });
  } else {
    const special = new Set(['sharedWifiNetworks', 'networkRoles', 'adminHotspotPassword']);
    const list = pageFields(settings!, route).filter(f => !special.has(f.key) && !['carlinkPictureDefault', 'carlinkPictureTry'].includes(f.key)).map(editorField);
    patchMarkup(container, `<section ${list.length ? '' : 'hidden'} class="inset-group settings-section" data-section="${route === 'settings/access/public-name' ? 'publicName' : route}">${list.map(fieldMarkup).join('')}</section>`);
    list.forEach(bindField);
    if (route === 'settings/connection') container.insertAdjacentHTML('beforeend', `<div class="inset-group">${menuRow('settings/connection/network', 'menu.connection/network', pageSummary('settings/connection/network'), 'wifi')}</div><div id="pairing-panel"></div>`);
    if (route === 'settings/connection/network') container.insertAdjacentHTML('beforeend', `<div id="network-panel" class="inset-group"></div>${status!.platform === 'linux' ? `<div class="inset-group">${menuRow('settings/connection/network/interfaces', 'menu.connection/network/interfaces', '', 'router')}</div>` : ''}`);
    if (route === 'settings/connection/network/interfaces') {
      container.insertAdjacentHTML('beforeend', connectionsView(status!, t) || `<div class="inset-group">${fields().filter(f => special.has(f.key) && f.key !== 'sharedWifiNetworks').map(fieldMarkup).join('')}</div>`);
      fields().filter(f => special.has(f.key) && f.key !== 'sharedWifiNetworks').forEach(f => { if (document.getElementById(fieldId(f.key))) bindField(f); }); bindConnections();
    }
    if (route === 'settings/access/public-name') container.insertAdjacentHTML('beforeend', '<div id="public-name-panel" class="inset-group"></div>');
    if (route === 'settings/access') {
      container.insertAdjacentHTML('beforeend', addressesView(status!, true, t) + `<div class="inset-group">${fields().some(f => f.key === 'publicNameEnabled') ? menuRow('settings/access/public-name', 'publicName.title', status!.publicName ? t(`publicName.state.${status!.publicName.state}`) : '', 'lock') : ''}${menuRow('settings/access/admin', 'menu.access/admin', defaultPassword ? t('home.passwordNote') : '', 'lock')}</div>`); bindCopies(container);
    }
    if (route === 'settings/access/admin') {
      container.insertAdjacentHTML('beforeend', `<section class="inset-group" id="password-card"><form id="password-form">${['current', 'new', 'confirm'].map(name => `<label for="${name}-password">${t(`ui.${name}Password`)}</label><input id="${name}-password" name="${name}" type="password" autocomplete="${name === 'current' ? 'current-password' : 'new-password'}" ${name === 'current' ? '' : 'minlength="8"'} required>`).join('')}<p class="muted">${t('error.passwordFormat')}</p><button class="primary">${t('ui.changePassword')}</button><p id="password-result" role="status"></p></form><button id="logout">${t('ui.logout')}</button></section>`);
      bindPassword(); on(document.getElementById('logout')!, 'click', async () => { try { await api.logout(); resetSession('login'); } catch (e) { pageError = errorKey(e); } });
    }
    if (route === 'settings/car' && status!.platform === 'linux') container.insertAdjacentHTML('beforeend', `<div class="inset-group"><div class="inset-row"><span>${t('car.savedScreen')} · ${esc(status!.pathInput?.car?.savedScreen ?? t('stage.noScreen'))}</span></div><button class="quiet full-width" id="clear-saved-head-unit">${t('car.clearSaved')}</button><p id="clear-saved-result" role="status"></p></div>`);
    on(document.getElementById('clear-saved-head-unit'), 'click', async () => { if (!await confirmSheet(t('carlink.clearSavedHint'), t('car.clearSaved'), t('ui.cancel'))) return; try { await api.action('clear-saved-head-unit'); document.getElementById('clear-saved-result')!.textContent = t('ui.saved'); } catch (e) { document.getElementById('clear-saved-result')!.textContent = t(errorKey(e)); } });
    if (route === 'settings/developer/diagnostics') { container.insertAdjacentHTML('beforeend', '<div id="diagnostics" class="inset-group"></div>'); renderDiagnostics(); }
    if (route === 'settings/developer/connection') {
      container.insertAdjacentHTML('beforeend', `<div class="inset-group"><button id="developer-restart">${t('home.restart')}</button><button id="forget-all-wifi">${t('net.forgetAll')}</button>${status!.platform === 'linux' ? `<button id="carlink-forget">${t('carlink.forget')}</button><button id="regenerate-code">${t('stage.regenerate')}</button>` : ''}</div>`);
      on(document.getElementById('developer-restart')!, 'click', async () => { if (await confirmSheet(t('home.restartHint'), t('home.restart'), t('ui.cancel'))) void api.action('restart').then(() => tick()); });
      on(document.getElementById('forget-all-wifi')!, 'click', () => void forgetAllWifi()); on(document.getElementById('carlink-forget')!, 'click', confirmForgetCarlink);
      on(document.getElementById('regenerate-code')!, 'click', () => void api.action('regenerate-pairing-code').then(() => tick()));
      const network = fields().find(f => f.key === 'networkMode'); if (network) { container.insertAdjacentHTML('beforeend', `<details class="inset-group"><summary>${t('stage.legacyNetwork')}</summary>${fieldMarkup(network)}</details>`); bindField(network); }
    }
    if (route === 'settings/developer/carlink') { container.insertAdjacentHTML('beforeend', `<details class="inset-group"><summary>${t('carlink.connect')}</summary><div id="context-action"></div></details>`); renderCarlink(); }
    if (route === 'settings/developer/carlink' && status!.platform === 'linux') {
      container.insertAdjacentHTML('beforeend', `<section class="inset-group" id="carlink-debug"><h2>${t('trace.title')}</h2><label for="trace-filter">${t('trace.filter')}</label><select id="trace-filter" data-trace-filter>${traceFilters.map((value, i) => `<option value="${value}">${t(['trace.all', 'trace.rtsp', 'trace.frames', 'trace.media', 'trace.link', 'trace.errors'][i])}</option>`).join('')}</select><p class="error" role="alert" data-trace-error></p><div data-trace-content></div></section>`);
      disposeTrace = mountTrace(document.getElementById('carlink-debug')!, t, errorKey);
    }
    if (route === 'settings/output' && pageAvailable(settings!, status!, 'settings/output/plugins')) container.insertAdjacentHTML('beforeend', `<div class="inset-group">${menuRow('settings/output/plugins', 'section.plugins', '', 'code')}</div>`);
    renderNetworkPanel(); renderPairingPanel(); updateVisibility(); renderPublicName();
  }
  syncPinDialog(); syncVisibleTimer();
}
async function copyStage(value: string) { const okay = await copyText(value, navigator.clipboard, window.isSecureContext, text => { const area = document.createElement('textarea'); area.value = text; document.body.append(area); area.select(); try { return document.execCommand('copy'); } finally { area.remove(); } }); if (!okay) networkDialog(t('ui.copy'), `<textarea readonly>${esc(value)}</textarea><button data-close>${t('ui.close')}</button>`); }
async function dispatchStage(action: StageAction) {
  if (!status || actionBusy) return;
  const command = stageCommand(action, status.platform, status.pathInput ? presentPath(status.pathInput).running : status.running); if (!command) return;
  switch (command.kind) {
    case 'route': showSettings(command.route); return;
    case 'phone': { const field = fields().find(f => f.key === 'iphone'); if (field) await pickPhone(field); return; }
    case 'pin': dismissedPin = null; syncPinDialog(); return;
    case 'network': editSharedNetwork(null); return;
    case 'hotspot': editHotspot(); return;
    case 'copy': void copyStage(status.pathInput?.car?.pageUrl ?? status.receiverUrls[0] ?? ''); return;
    case 'cancel': return;
    case 'api':
      if (command.endpoint === 'actions/pair-iphone') { await pairingAction('pair-iphone'); return; }
      actionBusy = true; updateDashboard();
      try { if (command.endpoint === 'carlink/connect') await api.carlinkConnect(); else await api.action(command.endpoint.slice(8) as 'start' | 'stop' | 'restart'); pageError = ''; await tick(); }
      catch (e) { pageError = errorKey(e); } finally { actionBusy = false; updateDashboard(); }
  }
}
function updateDashboard() {
  if (!status || !stage) { if (status && document.getElementById('context-action')) renderCarlink(); syncPinDialog(); syncVisibleTimer(); return; }
  stage.update(status.pathInput ?? { platform: status.platform, version: status.version }, { busy: actionBusy, displayChanged: needsRestart || restartRequired(status.notices), carIncomplete: !status.pathInput?.car?.screen && !status.pathInput?.car?.savedScreen && !settings?.values.carBrand && !settings?.values.carModel }, pageError);
  syncPinDialog(); syncVisibleTimer();
}
function syncVisibleTimer() {
  const visible = !document.hidden && (!!document.querySelector('#pairing-remaining, #carlink-countdown') || !!stage?.timerVisible || currentView === 'home' && videoStatsInterval(settings, status?.platform) > 0);
  if (visible && !visibleTimer) visibleTimer = setInterval(() => {
    stage?.tick(); syncPinDialog();
    if (currentView === 'home' && videoStatsInterval(settings, status?.platform) > 0) void tick();
    const seconds = Math.max(0, Math.ceil((status?.pairingDeadline ?? 0) - Date.now() / 1000));
    const panel = document.getElementById('pairing-remaining'); if (panel) panel.textContent = t('pairing.open', { time: formatRemaining(seconds) });
    if (!seconds && status?.pairingDeadline) { status.pairingDeadline = null; renderPairingPanel(); }
    syncVisibleTimer();
  }, 1000);
  else if (!visible) { clearInterval(visibleTimer); visibleTimer = undefined; }
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
  patchMarkup(el, pairingPanelView(s, pairingBusy, pairingError, t));
  on(document.getElementById('pairing-open'), 'click', () => void pairingAction('pair-iphone'));
  on(document.getElementById('pairing-cancel'), 'click', () => void pairingAction('cancel-pairing'));
  if (focused) (document.getElementById(focused) ?? document.getElementById('pairing-cancel') ?? document.getElementById('pairing-open'))?.focus();
}
function renderPublicName() {
  const el = document.getElementById('public-name-panel'); if (!el || !status || !settings) return;
  const v = settings.values;
  const signature = JSON.stringify([status.publicName, status.receiverUrls, v.publicNameEnabled, v.publicNameTokenSet, v.publicName, v.publicNameProvider, publicNameBusy, publicNameMessage, language]);
  if (el.dataset.signature === signature) return;
  const focused = el.contains(document.activeElement) ? (document.activeElement as HTMLElement).id : '';
  el.dataset.signature = signature;
  patchMarkup(el, publicNamePanel(status, settings, publicNameBusy, publicNameMessage, t, language));
  on(document.getElementById('public-name-renew'), 'click', () => void publicNameAction('public-name-renew'));
  on(document.getElementById('public-name-remove'), 'click', confirmRemovePublicName);
  on(document.getElementById('public-name-import'), 'click', () => document.getElementById('public-name-file')?.click());
  on(document.getElementById('public-name-file'), 'change', event => {
    const file = (event.target as HTMLInputElement).files?.[0];
    if (file) void importPublicName(file);
  });
  document.querySelectorAll<HTMLInputElement>('[data-section="publicName"] .field input, [data-section="publicName"] .field button').forEach(input => { input.disabled = publicNameBusy; });
  if (focused) (document.getElementById(focused) ?? document.getElementById(fieldId('publicNameEnabled')))?.focus();
}
async function publicNameAction(name: 'public-name-renew' | 'public-name-remove') {
  if (publicNameBusy) return;
  const current = epoch;
  publicNameBusy = true; publicNameMessage = ''; renderPublicName();
  try {
    if (name === 'public-name-remove') await settlePublicNameWrite();
    else await publicNameWrite?.catch(() => {});
    await api.action(name);
    const [nextStatus, nextSettings] = await Promise.all([fetchStatus(), name === 'public-name-remove' ? api.settings() : Promise.resolve(null)]);
    if (current !== epoch) return;
    status = nextStatus;
    if (nextSettings) {
      // The file is gone: unsaved edits of its fields go with it, others stay.
      for (const [key, item] of [...pending]) { if (key.startsWith('publicName')) { clearTimeout(item.timer); pending.delete(key); feedback.delete(key); } else nextSettings.values[key] = item.value; }
      publicNameDrafts = new PublicNameDrafts(); settings = nextSettings; publicNameBusy = false; render(); document.getElementById(fieldId('publicNameEnabled'))?.focus();
    }
  } catch (error) { if (current === epoch) publicNameMessage = errorKey(error); }
  finally { if (current === epoch) { publicNameBusy = false; renderPublicName(); } }
}
async function settlePublicNameWrite() {
  for (const [key, item] of [...pending]) if (key.startsWith('publicName')) { clearTimeout(item.timer); pending.delete(key); feedback.delete(key); }
  await publicNameWrite?.catch(() => {});
}
async function importPublicName(file: File) {
  if (publicNameBusy) return;
  const current = epoch;
  publicNameBusy = true; publicNameMessage = ''; renderPublicName();
  try {
    await publicNameWrite?.catch(() => {});
    const imported = await importPublicNameFile(file);
    const [nextStatus, nextSettings] = await Promise.all([fetchStatus(), api.settings()]);
    if (current !== epoch) return;
    await settlePublicNameWrite();
    for (const [key, item] of pending) nextSettings.values[key] = item.value;
    publicNameDrafts.imported(imported.provider, nextSettings.values); settings = nextSettings; status = nextStatus;
    publicNameMessage = imported; render();
  } catch (error) {
    if (current === epoch) publicNameMessage = error instanceof ApiError && Object.keys(error.errors).length ? { errors: error.errors } : errorKey(error);
  } finally { if (current === epoch) { publicNameBusy = false; renderPublicName(); } }
}
function confirmRemovePublicName() {
  const dialog = document.createElement('dialog'); dialog.setAttribute('aria-labelledby', 'public-name-remove-title');
  patchMarkup(dialog, `<h2 id="public-name-remove-title">${t('publicName.remove')}</h2><p>${t('publicName.removeHint')}</p><div class="dialog-actions"><button class="cancel">${t('ui.cancel')}</button><button class="confirm">${t('publicName.remove')}</button></div>`);
  document.body.append(dialog); dialog.showModal();
  const close = () => { dialog.close(); dialog.remove(); document.getElementById('public-name-remove')?.focus(); };
  on(dialog.querySelector('.cancel')!, 'click', close);
  on(dialog, 'cancel', event => { event.preventDefault(); close(); });
  on(dialog.querySelector('.confirm')!, 'click', () => { close(); void publicNameAction('public-name-remove'); });
}
function renderDiagnostics() {
  const el = document.getElementById('diagnostics'); if (!el || !status) return;
  const signature = JSON.stringify([status.carlink, status.video, status.receivers, status.receiverHistory, status.bluetoothGuard, status.hostProfile, status.hostCapabilities, status.wireless, status.platform === 'linux' ? status.network : null, language]);
  if (el.dataset.signature === signature) return;
  el.dataset.signature = signature;
  const open = new Set([...el.querySelectorAll<HTMLDetailsElement>('details[data-details]')].filter(d => d.open).map(d => d.dataset.details));
  const focused = (document.activeElement as HTMLElement | null)?.dataset.report;
  patchMarkup(el, diagnosticsView(status, t));
  el.querySelectorAll<HTMLDetailsElement>('details[data-details]').forEach(d => { d.open = open.has(d.dataset.details); });
  el.querySelectorAll<HTMLButtonElement>('[data-report]').forEach(button => {
    const feedback = button.parentElement!.querySelector<HTMLElement>('.report-feedback')!;
    if (reportCopied && reportCopied.client === button.dataset.report && reportCopied.until > Date.now()) feedback.textContent = t(reportCopied.key);
    on(button, 'click', () => void copyReport(button.dataset.report!));
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
    on(button, 'click', async () => {
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
  scope.querySelectorAll<HTMLButtonElement>('[data-secret-toggle]').forEach(button => on(button, 'click', () => {
    const text = button.parentElement!.querySelector<HTMLElement>('[data-secret]')!;
    const reveal = button.getAttribute('aria-pressed') !== 'true';
    text.textContent = reveal ? text.dataset.secret! : '••••••••';
    button.setAttribute('aria-pressed', String(reveal)); button.setAttribute('aria-label', t(reveal ? 'ui.hide' : 'ui.reveal'));
    patchMarkup(button, icon(reveal ? 'eye-off' : 'eye'));
  }));
}
function renderCarlink() {
  if (!status || !document.getElementById('context-action') || (!carlinkView(status).visible && status.output !== 'multiple')) { syncPinDialog(); return; }
  const c = status.carlink;
  patchMarkup(document.getElementById('context-action')!, carlinkConnectView(status, connectPin, connectBusy, carlinkError, t));
  const connectInput = document.getElementById('carlink-connect-pin') as HTMLInputElement;
  on(connectInput, 'input', () => { connectPin = connectInput.value; });
  on(document.getElementById('carlink-connect-form')!, 'submit', async event => {
    event.preventDefault();
    if (connectBusy) return;
    if (!canConnectCarlink(connectPin, c)) { carlinkError = 'error.carlinkPin'; updateDashboard(); return; }
    connectBusy = true; const current = epoch;
    const pin = connectPin; connectPin = ''; updateDashboard();
    try { await api.carlinkConnect(pin || undefined); if (current === epoch) { carlinkError = ''; void tick(); } }
    catch (error) { if (current === epoch) carlinkError = errorKey(error); }
    finally { if (current === epoch) { connectBusy = false; updateDashboard(); } }
  });
  on(document.getElementById('carlink-enter-pin'), 'click', () => { dismissedPin = null; syncPinDialog(); });
  on(document.getElementById('carlink-forget'), 'click', confirmForgetCarlink);
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
    patchMarkup(dialog, `<form><h2 id="carlink-pin-title">${t('carlink.enterPin')}</h2><p>${t('carlink.pinHint')}</p><label for="carlink-pin">${t('carlink.pin')}</label><input id="carlink-pin" name="pin" type="text" inputmode="numeric" pattern="[0-9]{4,8}" minlength="4" maxlength="8" autocomplete="off" required aria-describedby="carlink-countdown carlink-pin-error"><p id="carlink-countdown" role="timer"></p><p id="carlink-pin-error" class="error" role="alert"></p><div class="dialog-actions"><button type="button" class="cancel">${t('ui.cancel')}</button><button type="submit" class="primary">${t('carlink.submitPin')}</button></div></form>`);
    document.body.append(dialog); dialog.showModal();
    const currentDialog = dialog;
    const close = () => { dismissedPin = deadline; currentDialog.close(); currentDialog.remove(); document.getElementById('carlink-enter-pin')?.focus(); };
    on(dialog.querySelector('.cancel')!, 'click', close);
    on(dialog, 'cancel', event => { event.preventDefault(); close(); });
    on(dialog.querySelector('form')!, 'submit', async event => {
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
          status.carlink.pinRequested = false; status.carlink.pinDeadline = null; status.carlink.pinIncorrect = false;
          carlinkError = ''; close(); dismissedPin = null; updateDashboard();
        }
      } catch (error) { errorEl.textContent = t(errorKey(error), error instanceof ApiError ? { seconds: error.retryAfter } : undefined); }
      finally { button.disabled = false; }
    });
  }
  if (carlinkPinError(status?.carlink)) dialog.querySelector('#carlink-pin-error')!.textContent = t(carlinkPinError(status?.carlink));
  dialog.querySelector('#carlink-countdown')!.textContent = t('carlink.countdown', { seconds: view.seconds });
}
function confirmForgetCarlink() {
  const dialog = document.createElement('dialog'); dialog.setAttribute('aria-labelledby', 'carlink-forget-title');
  patchMarkup(dialog, `<h2 id="carlink-forget-title">${t('carlink.forget')}</h2><p>${t('carlink.forgetHint')}</p><p class="error" role="alert"></p><div class="dialog-actions"><button class="cancel">${t('ui.cancel')}</button><button class="confirm">${t('carlink.forget')}</button></div>`);
  document.body.append(dialog); dialog.showModal();
  const close = () => { dialog.close(); dialog.remove(); document.getElementById('carlink-forget')?.focus(); };
  on(dialog.querySelector('.cancel')!, 'click', close);
  on(dialog, 'cancel', event => { event.preventDefault(); close(); });
  on(dialog.querySelector('.confirm')!, 'click', async event => {
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
  field = editorField(field);
  const id = fieldId(field.key), value = settings!.values[field.key], draft = drafts.get(field.key);
  const label = esc(t(field.labelKey));
  const attr = `id="${id}" aria-describedby="${id}-hint ${id}-result"`;
  let control = '';
  if (field.key === 'carlinkPictureSizes') control = `<div ${attr}></div>`;
  else if (field.type === 'plugins') control = `<div ${attr}>${pluginList(settings!, language, t, true)}</div>`;
  else if (field.type === 'bool' || field.key === 'carlinkTrace') control = `<label class="switch-row" for="${id}"><span>${label}</span><input ${attr} type="checkbox" role="switch" ${value === true || value === 'on' ? 'checked' : ''}><span class="switch-track" aria-hidden="true"></span></label>`;
  else if (field.type === 'choice' || field.type === 'phone') {
    const summary = field.key === 'frameRate' && value === 'auto' ? autoFrameRateSummary(status!, t) : field.type === 'phone' ? value === 'auto' ? t('ui.automatic') : phoneNames.get(String(value)) ?? value : optionLabel(field.options?.find(o => o.value === value), language, t);
    control = `<button ${attr} class="inset-row picker-button" aria-haspopup="dialog"><span class="row-copy"><span>${label}</span><small class="muted">${esc(summary)}</small></span><span aria-hidden="true">›</span></button>`;
  } else {
    const object = field.type === 'object' || typeof value === 'object' && value !== null;
    const string = draft ?? (object ? JSON.stringify(value, null, 2) : String(value ?? ''));
    control = `<label for="${id}">${label}</label><div class="input-row">${object ? `<textarea ${attr} rows="4">${esc(string)}</textarea>` : `<input ${attr} type="${field.type === 'secret' ? 'password' : field.type === 'number' ? 'number' : 'text'}" value="${esc(string)}" ${field.min !== undefined ? `min="${field.min}"` : ''} ${field.max !== undefined ? `max="${field.max}"` : ''} ${field.type === 'number' ? 'step="1" inputmode="numeric"' : ''} ${field.type === 'secret' ? 'autocomplete="new-password"' : ''}>`}${field.type === 'secret' ? `<button type="button" class="icon-button reveal" aria-label="${t('ui.reveal')}">${icon('eye')}</button>` : ''}</div><div class="field-edit-actions" ${draft === undefined ? 'hidden' : ''}><button type="button" data-save="${esc(field.key)}">${t('net.save')}</button><button type="button" data-cancel="${esc(field.key)}">${t('ui.cancel')}</button></div>`;
  }
  const hint = field.key === 'publicNameProvider' ? publicNameGuidance(settings!.values, t, language) : field.key === 'publicName' ? esc(t(publicNameNameHint(settings!.values))) : field.hintKey ? esc(t(field.hintKey)) : '';
  return `<div class="field" data-field="${esc(field.key)}">${control}<p class="muted hint" id="${id}-hint">${hint}</p><p id="${id}-result" class="field-result" role="status">${esc(t(feedback.get(field.key) ?? ''))}</p></div>`;
}
function bindField(field: Field) {
  field = editorField(field);
  const el = document.getElementById(fieldId(field.key)); if (!el) return;
  if (['carlinkPictureSizes', 'carlinkPictureDefault', 'carlinkPictureTry'].includes(field.key)) { mountPictureField(el, field, settings!.values, t, async changes => { const response = await api.save(changes); needsRestart ||= restartRequired([], response.applied); renderRestart(); }, status!.pathInput?.car?.carlink?.picture ?? undefined); return; }
  if (field.type === 'plugins') on(el, 'change', event => { const input = event.target as HTMLInputElement; queueValue(field, togglePlugin(settings!.values.pluginsEnabled, input.dataset.plugin!, input.checked)); });
  else if (field.type === 'bool' || field.key === 'carlinkTrace') on(el, 'change', () => queueSave(field, field.key === 'carlinkTrace' ? (el as HTMLInputElement).checked ? 'on' : 'off' : (el as HTMLInputElement).checked));
  else if (field.type === 'phone') on(el, 'click', () => void pickPhone(field));
  else if (field.type === 'choice') on(el, 'click', () => {
    const { dialog, close } = networkDialog(t(field.labelKey), `<div class="${['frameRate', 'resolution', 'carplayResolution', 'videoCodec'].includes(field.key) ? 'chips' : 'choice-list'}" role="group">${field.options?.map((option, index) => `<button data-option="${index}" aria-pressed="${option.value === settings!.values[field.key]}" ${field.key === 'networkMode' ? networkOptionState(status?.platform, settings!.values, option.value) : ''}><span>${esc(optionLabel(option, language, t))}</span>${option.value === settings!.values[field.key] ? icon('check') : ''}${option.hintKey ? `<small class="muted">${esc(t(option.hintKey))}</small>` : ''}</button>`).join('')}</div><button data-close>${t('ui.cancel')}</button>`);
    dialog.querySelectorAll<HTMLButtonElement>('[data-option]').forEach(button => on(button, 'click', async () => {
      const option = field.options![Number(button.dataset.option)];
      if (restartsPicture(field.key) && status?.running && !await confirmSheet(t('files.pictureRestart'), t('ui.save'), t('ui.cancel'))) return;
      close();
      if (field.key === 'networkMode') { requestNetworkSwitch(String(option.value)); return; }
      queueSave(field, String(option.value));
      el.querySelector('small')!.textContent = optionLabel(option, language, t);
    }));
  });
  else {
    const input = el as HTMLInputElement | HTMLTextAreaElement;
    const actions = el.closest('.field')!.querySelector<HTMLElement>('.field-edit-actions')!;
    on(el, 'input', () => { const stored = settings!.values[field.key]; const baseline = typeof stored === 'object' && stored !== null ? JSON.stringify(stored, null, 2) : String(stored ?? ''); if (input.value === baseline) drafts.delete(field.key); else drafts.set(field.key, input.value); actions.hidden = !drafts.has(field.key); });
    on(actions.querySelector('[data-cancel]')!, 'click', () => { drafts.delete(field.key); input.value = typeof settings!.values[field.key] === 'object' ? JSON.stringify(settings!.values[field.key], null, 2) : String(settings!.values[field.key] ?? ''); actions.hidden = true; result(field.key, ''); });
    on(actions.querySelector('[data-save]')!, 'click', async () => {
      if (!input.checkValidity()) { result(field.key, 'error.range'); input.reportValidity(); return; }
      if (restartsPicture(field.key) && status?.running && !await confirmSheet(t('files.pictureRestart'), t('ui.save'), t('ui.cancel'))) return;
      if (field.type === 'object') { try { const value = JSON.parse(input.value); queueValue(field, value); } catch { result(field.key, 'error.invalid'); return; } }
      else queueSave(field, input.value);
      drafts.delete(field.key); actions.hidden = true;
    });
  }
  if (field.type === 'secret') on(el.parentElement!.querySelector('button')!, 'click', event => {
    const input = el as HTMLInputElement, reveal = input.type === 'password'; input.type = reveal ? 'text' : 'password';
    const button = event.currentTarget as HTMLButtonElement; button.setAttribute('aria-pressed', String(reveal)); button.setAttribute('aria-label', t(reveal ? 'ui.hide' : 'ui.reveal')); patchMarkup(button, icon(reveal ? 'eye-off' : 'eye'));
  });
}
function networkDialog(title: string, body: string) {
  const trigger = document.activeElement as HTMLElement | null;
  const triggerId = trigger?.id;
  const dialog = document.createElement('dialog'); dialog.setAttribute('aria-labelledby', 'network-dialog-title');
  patchMarkup(dialog, `<h2 id="network-dialog-title">${esc(title)}</h2>${body}`);
  document.body.append(dialog); dialog.showModal();
  const close = () => {
    dialog.close(); dialog.remove();
    (trigger?.isConnected ? trigger : triggerId ? document.getElementById(triggerId) : null)?.focus();
    if (!document.activeElement || document.activeElement === document.body) document.getElementById('network-add')?.focus();
  };
  on(dialog, 'cancel', event => { event.preventDefault(); close(); });
  dialog.querySelectorAll('[data-close]').forEach(button => on(button, 'click', close));
  bindCopies(dialog);
  return { dialog, close };
}
function renderRestart() {
  const el = document.getElementById('restart-required'); if (!el || !status) return;
  const required = needsRestart || restartRequired(status.notices);
  el.hidden = !required || currentView === 'home';
  const signature = `${required}:${language}`;
  if (el.dataset.signature === signature) return;
  el.dataset.signature = signature; patchMarkup(el, restartView(required, t));
  on(el.querySelector<HTMLButtonElement>('#restart-now'), 'click', async event => {
    if (!await confirmSheet(t('home.restartHint'), t('home.restart'), t('ui.cancel'))) return;
    const button = event.currentTarget as HTMLButtonElement; button.disabled = true;
    try { await api.action('restart'); needsRestart = false; void tick(); }
    catch (error) { el.querySelector('#restart-error')!.textContent = t(errorKey(error)); }
    finally { button.disabled = false; }
  });
}
function editHotspot() {
  if (!status?.hotspot) return;
  const { dialog, close } = networkDialog(t('hotspot.change'), hotspotForm(status.hotspot.ssid, t, status.wireless));
  const name = dialog.querySelector<HTMLInputElement>('#hotspot-name')!;
  const password = dialog.querySelector<HTMLInputElement>('#hotspot-password')!;
  on(dialog.querySelector('#hotspot-reveal')!, 'click', event => {
    const reveal = password.type === 'password'; password.type = reveal ? 'text' : 'password';
    const button = event.currentTarget as HTMLButtonElement;
    button.setAttribute('aria-pressed', String(reveal)); button.setAttribute('aria-label', t(reveal ? 'ui.hide' : 'ui.reveal')); patchMarkup(button, icon(reveal ? 'eye-off' : 'eye'));
  });
  on(dialog.querySelector('form')!, 'submit', event => {
    event.preventDefault();
    const error = validateHotspot(name.value, password.value);
    dialog.querySelector('#hotspot-error')!.textContent = t(error);
    name.setAttribute('aria-invalid', String(error === 'error.hotspotSsid'));
    password.setAttribute('aria-invalid', String(error === 'error.hotspotPassword'));
    if (error) return;
    const ssid = name.value, secret = password.value;
    const band = dialog.querySelector<HTMLSelectElement>('#hotspot-band')!.value as HotspotBand;
    const active = status!.network.mode === 'device-hotspot';
    // Keep the form open underneath so Cancel returns to the entered values.
    const confirmation = networkDialog(t('hotspot.confirm'), `<p>${esc(t(active ? 'hotspot.warning' : 'hotspot.later'))}</p><p>${esc(ssid)}</p><p class="error" role="alert"></p><div class="dialog-actions"><button data-close>${t('ui.cancel')}</button><button data-confirm class="primary">${t('hotspot.apply')}</button></div>`);
    on(confirmation.dialog.querySelector<HTMLButtonElement>('[data-confirm]')!, 'click', async event => {
      const button = event.currentTarget as HTMLButtonElement; button.disabled = true;
      try {
        await api.hotspot(ssid, secret, band);
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
  const signature = JSON.stringify([v, status.network.followingCarlink, status.hotspot, status.wireless, status.wifiControl, networks, networkError, networkAnnouncement, language]);
  if (el.dataset.signature === signature) return;
  const focused = el.contains(document.activeElement) ? (document.activeElement as HTMLElement).id : '';
  el.dataset.signature = signature;
  patchMarkup(el, `<div class="field"><h3>${t('net.title')}</h3>${connectionConstraint(status?.platform, t)}${status.network.followingCarlink ? `<p>${t('network.followingCarlink')}</p>` : ''}<div class="network-current"><h3>${t(v.label)}</h3><p role="status" aria-live="polite">${esc(t(v.statusKey, v.parameters))}</p>${topology(v.mode as Illustration)}${!v.shared || status.wifiClient?.setupHotspot ? hotspotDetails(status, t) : ''}${!v.shared && status.platform === 'linux' ? `<button id="hotspot-edit" aria-haspopup="dialog">${t('hotspot.change')}</button>` : ''}${v.receiver && !v.shared ? `<p>${esc(t('net.carHint', { url: v.receiver }))}</p>` : ''}</div><p>${t(v.shared ? 'net.useHotspot' : 'net.useShared')}</p><button id="network-switch" ${networkOptionState(status?.platform, settings!.values, v.target)} aria-haspopup="dialog">${t(v.shared ? 'net.switchHotspot' : v.needsNetwork ? 'net.addSwitch' : 'net.switchShared')}</button><p id="network-mode-result" class="error" role="alert">${esc(t(networkError))}</p><p id="network-announcement" role="status" aria-live="polite"></p><h3>${t('net.saved')}</h3><p class="muted">${t('net.note')}</p><div id="saved-networks">${v.list.map((entry, index) => `<button class="phone-row" id="saved-network-${index}" data-network="${index}" aria-haspopup="dialog"><span>${esc(entry.ssid)}</span><span class="badges">${savedNetworkTags(entry, status!, networks).map(key => `<span class="tag">${t(key)}</span>`).join('')}</span></button>`).join('')}</div><button id="network-add" ${v.full ? 'disabled' : ''} aria-haspopup="dialog">${t('net.add')}</button>${v.full ? `<p>${t('net.limit')}</p>` : ''}</div>`);
  on(document.getElementById('hotspot-edit'), 'click', editHotspot);
  const switchButton = document.getElementById('network-switch')!;
  if (status.platform === 'linux' && status.network.plan) {
    switchButton.previousElementSibling?.remove();
    switchButton.remove();
  } else on(switchButton, 'click', () => requestNetworkSwitch(v.target));
  on(document.getElementById('network-add')!, 'click', () => editSharedNetwork(null));
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
  document.querySelectorAll<HTMLButtonElement>('[data-network]').forEach(button => on(button, 'click', () => {
    const entry = savedNetworks(settings!)[Number(button.dataset.network)];
    const { dialog, close } = networkDialog(entry.ssid, `<div class="network-actions">${canConnectNetwork(entry, status!) ? `<button data-connect>${t('net.connect')}</button>` : ''}<button data-edit>${t('net.change')}</button><button data-delete>${t('net.delete')}</button><button data-close>${t('ui.close')}</button></div>`);
    on(dialog.querySelector('[data-edit]')!, 'click', () => { close(); editSharedNetwork(entry); });
    on(dialog.querySelector('[data-connect]'), 'click', () => {
      close();
      const consequences = switchConsequences(status!, settings!, 'shared-wifi'); consequences.parameters.wifi = entry.ssid; consequences.change = 'net.joinWifi';
      void beginNetworkSwitch('shared-wifi', consequences, () => api.wifi(entry.ssid));
    });
    on(dialog.querySelector('[data-delete]')!, 'click', () => {
      close();
      const current = savedNetworkTags(entry, status!, networks).includes('net.current');
      const confirmation = networkDialog(t('net.deleteTitle', { ssid: entry.ssid }), `${current ? `<p>${t('net.disconnect')}</p>` : ''}<p class="error" role="alert"></p><div class="dialog-actions"><button data-close>${t('ui.cancel')}</button><button data-confirm>${t('net.delete')}</button></div>`);
      on(confirmation.dialog.querySelector('[data-confirm]')!, 'click', async event => {
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
  const { dialog, close } = networkDialog(t(entry ? 'net.change' : 'net.add'), `<form><label for="network-name">${t('net.wifiName')}</label><input id="network-name" value="${esc(entry?.ssid ?? '')}" ${entry ? 'readonly' : ''} required autocomplete="off">${!entry ? stationBandHint(status, t) : ''}${!entry && status.wifiControl ? `<section class="nearby"><div class="nearby-heading"><h3>${t('net.scan')}</h3><button type="button" id="network-refresh" class="icon-button" aria-label="${t('net.refresh')}" title="${t('net.refresh')}">${icon('refresh-cw')}</button></div><p id="network-scan-status" role="status"></p><p class="muted" ${status.wifiScanStale || status.hotspot?.active || status.wifiClient?.setupHotspot ? '' : 'hidden'}>${t('net.stale')}</p><div id="network-scan"></div></section>` : ''}<div id="network-secret"><label for="network-password">${t('net.password')}</label><div class="input-row"><input id="network-password" type="password" autocomplete="new-password" placeholder="${entry?.hasPassword ? t('networks.keep') : ''}"><button type="button" id="network-reveal" class="icon-button" aria-label="${t('ui.reveal')}" aria-pressed="false">${icon('eye')}</button></div></div><label class="check-row"><input id="network-open" type="checkbox" ${entry && !entry.hasPassword ? 'checked' : ''}>${t('net.open')}</label><p id="network-error" class="error" role="alert"></p>${connect ? `<p>${t('net.saveConnect')}</p>` : ''}<div class="dialog-actions"><button type="button" data-close>${t('ui.cancel')}</button><button type="submit" class="primary">${t('net.save')}</button></div></form>`);
  const name = dialog.querySelector<HTMLInputElement>('#network-name')!;
  const password = dialog.querySelector<HTMLInputElement>('#network-password')!;
  const open = dialog.querySelector<HTMLInputElement>('#network-open')!;
  const syncSecret = () => { dialog.querySelector<HTMLElement>('#network-secret')!.hidden = open.checked; };
  on(open, 'change', syncSecret); syncSecret();
  on(dialog.querySelector('#network-reveal')!, 'click', event => {
    const reveal = password.type === 'password'; password.type = reveal ? 'text' : 'password';
    const button = event.currentTarget as HTMLButtonElement; button.setAttribute('aria-pressed', String(reveal)); button.setAttribute('aria-label', t(reveal ? 'ui.hide' : 'ui.reveal')); patchMarkup(button, icon(reveal ? 'eye-off' : 'eye'));
  });
  const scan = async () => {
    const button = dialog.querySelector<HTMLButtonElement>('#network-refresh'); if (!button) return;
    button.disabled = true;
    const feedback = dialog.querySelector('#network-scan-status')!; feedback.textContent = t('ui.loading');
    try {
      const found = sortNetworks(await api.networks()); if (!dialog.isConnected) return;
      networks = found; renderNetworkPanel(); feedback.textContent = found.length ? '' : t('wifi.empty');
      const list = dialog.querySelector('#network-scan')!;
      patchMarkup(list, nearbyNetworksView(found, t));
      list.querySelectorAll<HTMLButtonElement>('button').forEach(button => on(button, 'click', () => {
        const network = found[Number(button.dataset.nearby)]; name.value = network.ssid; password.value = ''; open.checked = !network.secure; syncSecret();
        (network.secure ? password : dialog.querySelector<HTMLButtonElement>('[type="submit"]'))?.focus();
      }));
    } catch (error) { feedback.textContent = t(errorKey(error)); }
    finally { button.disabled = false; }
  };
  on(dialog.querySelector('#network-refresh'), 'click', () => void scan());
  if (!entry && status.wifiControl) void scan();
  on(dialog.querySelector('form')!, 'submit', async event => {
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
  on(dialog.querySelector('[data-confirm]')!, 'click', () => { close(); void beginNetworkSwitch(target, c, () => api.switchNetwork(target)); });
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
  patchMarkup(document.getElementById('main')!, `<section class="card" id="network-switching"><h1 tabindex="-1" role="status" aria-live="polite">${t('net.switching')}</h1><p>${esc(t(c.change, c.parameters))}</p><p>${t('net.iphone')}</p><p>${esc(t(c.car, c.parameters))}</p>${!c.shared && status ? hotspotDetails(status, t) : ''}<p>${esc(t(c.comeback, c.parameters))}</p></section>`);
  bindCopies(document.getElementById('network-switching')!);
}

function updateVisibility() {
  for (const field of fields()) {
    const el = document.getElementById(fieldId(field.key))?.closest<HTMLElement>('.field');
    if (el) el.hidden = !isVisible(field, settings!.values) && !['networkRoles', 'wifiInterface', 'carlinkInterface', 'wifiCountry', 'adminHotspotPassword'].includes(field.key);
  }
  document.querySelectorAll<HTMLElement>('.settings-section').forEach(section => { section.hidden = ![...section.children].some(child => !(child as HTMLElement).hidden); });
  renderPublicName();
}
async function queueSave(field: Field, raw: string | boolean) {
  const value = coerceValue(field, raw);
  if (field.key.startsWith('publicName')) {
    if (publicNameBusy) return;
    if (field.key === 'publicNameProvider') publicNameDrafts.select(String(value), settings!.values);
    else {
      settings!.values[field.key] = value;
      if (field.key === 'publicName' || field.key === 'publicNameToken') publicNameDrafts.edit(String(settings!.values.publicNameProvider ?? ''), settings!.values);
    }
    // One serialized save submits the selected provider and its fields together.
    queueValue(fields().find(f => f.key === 'publicNameProvider')!, settings!.values.publicNameProvider);
    if (field.key === 'publicNameProvider') {
      render(); document.querySelector<HTMLInputElement>(`input[name="${fieldId(field.key)}"]:checked`)?.focus();
    }
    return;
  }
  if (field.key === 'transport' && needsUsbConfirmation(status?.platform, settings!.values.transport, value)) {
    if (!await confirmSheet(t('confirm.transport.usb.android'), t('ui.continue'), t('ui.cancel'))) { render(); return; }
  }
  if (field.key === 'output' && needsHotspotConfirmation(status?.platform, settings!.values, value)) {
    if (!await confirmSheet(t('constraint.carlinkWifiDirectConfirm'), t('ui.continue'), t('ui.cancel'))) { render(); return; }
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
  if (key.startsWith('publicName') && publicNameBusy) { item.timer = setTimeout(() => void save(key), 2000); return; }
  if (switching) { item.timer = setTimeout(() => void save(key), 2000); return; }
  item.busy = true; const revision = item.revision; const current = epoch;
  const submitted = key === 'publicNameProvider' ? publicNameSubmission(settings!.values) : null;
  try {
    const write = key === 'wifiCountry' ? api.saveRoles(settings!.values.networkRoles as NetworkRolesSetting, { wifiCountry: String(item.value) }).then(() => ({ applied: 'now' as const })) : api.save(submitted ?? { [key]: key === 'sharedWifiNetworks' ? sharedNetworkUpdate(item.value as SavedNetwork[]) : item.value });
    if (submitted) publicNameWrite = write;
    const response = await write;
    if (current !== epoch) return;
    if (submitted && settings) {
      publicNameDrafts.saved(submitted, settings.values);
      const token = document.getElementById(fieldId('publicNameToken')) as HTMLInputElement | null;
      if (token) { token.value = String(settings.values.publicNameToken ?? ''); token.placeholder = settings.values.publicNameTokenSet ? t('publicName.tokenSaved') : ''; }
      for (const field of fields().filter(f => f.key.startsWith('publicName'))) result(field.key, 'ui.saved');
    }
    if (revision === item.revision) { pending.delete(key); if (['carBrand', 'carModel', 'carYear'].includes(key)) { const fresh = await api.settings(); for (const [pendingKey, pendingItem] of pending) fresh.values[pendingKey] = pendingItem.value; settings = fresh; render(); } needsRestart ||= restartRequired([], response.applied); renderRestart(); result(key, response.applied === 'now' ? 'ui.saved' : 'ui.restartRequired'); }
  } catch (error) {
    if (current !== epoch) return;
    if (revision === item.revision) {
      if (error instanceof NetworkError) result(key, 'ui.unsaved');
      else { pending.delete(key); result(key, error instanceof ApiError ? error.errors[key] ?? error.key : 'error.request'); }
      if (submitted && error instanceof ApiError) for (const [field, message] of Object.entries(error.errors)) result(field, message);
    }
  } finally {
    item.busy = false;
    if (submitted && current === epoch) publicNameWrite = null;
    if (pending.get(key) === item && current === epoch) item.timer = setTimeout(() => void save(key), revision === item.revision ? 2000 : 450);
  }
}
async function pickPhone(field: Field) {
  const trigger = document.getElementById(fieldId(field.key)) ?? document.getElementById('stage-primary')!;
  const dialog = document.createElement('dialog'); dialog.setAttribute('aria-labelledby', 'phone-title');
  patchMarkup(dialog, `<div class="sheet-header"><h2 id="phone-title">${t('ui.choosePhone')}</h2><button class="icon-button close" aria-label="${t('ui.close')}">${icon('x')}</button></div><div id="phone-list" role="status">${t('ui.loading')}</div><button type="button" class="quiet full-width" data-phone-cancel>${t('ui.cancel')}</button>`);
  document.body.append(dialog); dialog.showModal();
  const close = () => { dialog.close(); dialog.remove(); trigger.focus(); };
  on(dialog.querySelector('.close')!, 'click', close);
  on(dialog.querySelector('[data-phone-cancel]'), 'click', close);
  on(dialog, 'cancel', event => { event.preventDefault(); close(); });
  const current = epoch;
  const list = dialog.querySelector<HTMLElement>('#phone-list')!;
  const populate = (phones: Phone[]) => {
    phones.forEach(phone => phoneNames.set(phone.id, phone.name));
    const rows: Phone[] = [{ id: 'auto', name: t('ui.automatic'), kind: 'phone', connected: false, likelyIphone: false, selected: settings!.values[field.key] === 'auto' }, ...phones];
    list.removeAttribute('role');
    patchMarkup(list, rows.map((phone, i) => `<button class="phone-row" data-index="${i}" aria-pressed="${settings!.values[field.key] === phone.id}">${icon(phone.id === 'auto' ? 'router' : phone.kind === 'car' ? 'car' : phone.kind === 'phone' ? 'smartphone' : 'bluetooth')}<span><strong>${esc(phone.name)}</strong><span class="badges">${phone.id !== 'auto' ? `<span>${esc(t(`ui.${phone.kind}`))}</span>` : ''}${phone.connected ? `<span class="tag">${t('ui.connected')}</span>` : ''}${phone.likelyIphone ? `<span class="tag">${t('ui.likelyIphone')}</span>` : ''}${phone.selected ? `<span>${t('ui.selected')}</span>` : ''}</span></span></button>`).join('') + (!phones.length ? `<p>${t('ui.noPhones')}</p>` : ''));
    list.querySelectorAll<HTMLButtonElement>('button').forEach(button => on(button, 'click', () => {
      const phone = rows[Number(button.dataset.index)]; queueSave(field, phone.id);
      const label = trigger.querySelector('small'); if (label) label.textContent = phone.name; close();
    }));
  };
  try { const phones = await api.phones(); if (current === epoch && dialog.isConnected) populate(phones); else close(); }
  catch (error) { list.textContent = t(errorKey(error)); }
}
function bindPassword() {
  on(document.getElementById('password-form')!, 'submit', async event => {
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
    patchMarkup(dialog, `<h2 id="forget-wifi-title">${t('net.forgetAll')}</h2><p>${t('home.forgetSavedWifi')}</p><p class="error" role="alert"></p><div class="dialog-actions"><button class="cancel">${t('ui.cancel')}</button><button class="confirm">${t('net.forgetAll')}</button></div>`);
    document.body.append(dialog); dialog.showModal();
    const close = () => { dialog.close(); dialog.remove(); trigger?.focus(); };
    on(dialog.querySelector('.cancel')!, 'click', close);
    on(dialog, 'cancel', event => { event.preventDefault(); close(); });
    on(dialog.querySelector('.confirm')!, 'click', async event => {
      const button = event.currentTarget as HTMLButtonElement; button.disabled = true;
      try { await api.save({ sharedWifiNetworks: [] }); settings!.values.sharedWifiNetworks = []; close(); render(); document.getElementById('forget-all-wifi')?.focus(); }
      catch (error) { dialog.querySelector('.error')!.textContent = t(errorKey(error)); }
      finally { button.disabled = false; }
    });
    return;
  }
  const dialog = document.createElement('dialog'); dialog.setAttribute('aria-labelledby', 'wifi-confirm-title');
  patchMarkup(dialog, `<h2 id="wifi-confirm-title">${t('net.forgetAll')}</h2><p>${esc(t(wifiGuidance('forget')))}</p><div class="dialog-actions"><button id="wifi-confirm" class="primary">${t('net.forgetAll')}</button><button id="wifi-cancel" class="quiet">${t('ui.cancel')}</button></div>`);
  document.body.append(dialog); dialog.showModal();
  const close = () => { dialog.close(); dialog.remove(); trigger?.focus(); };
  on(dialog.querySelector('#wifi-cancel')!, 'click', close);
  on(dialog, 'cancel', event => { event.preventDefault(); close(); });
  on(dialog.querySelector('#wifi-confirm')!, 'click', async () => {
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
  patchMarkup(document.getElementById('main')!, `<section class="card auth"><h1>${t('wifi.reconnect')}</h1><p role="status">${esc(t(wifiGuidance(handoff!.action), { ssid: handoff!.ssid }))}</p>${handoff!.error ? `<p class="error" role="alert">${esc(t(handoff!.error!))}</p>` : ''}<p>${t('wifi.reloadHint')}</p></section>`);
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
      if (current === epoch && session === 'ready') { status = next;
        if (currentView === 'home') videoStats.read(status, videoStatsInterval(settings, status.platform), performance.now());
        renderRestart();
        if (switching && next.network.mode === switching.target && Date.now() - switching.started >= 2000) {
          const joinError = next.notices.find(n => n.startsWith('wifi.joinFailed.'));
          networkError = joinError ?? '';
          networkAnnouncement = joinError ? '' : t('net.switched', { mode: t(next.network.mode === 'shared-wifi' ? 'net.shared' : 'net.hotspot') });
          settings = await api.settings();
          for (const [key, item] of pending) settings.values[key] = item.value;
          if (pending.has('publicNameProvider')) publicNameDrafts.select(String(settings.values.publicNameProvider), settings.values);
          switching = null; render(); document.getElementById('network-switch')?.focus();
        } else if (!switching) { updateDashboard(); renderNetworkPanel(); renderPairingPanel(); renderDiagnostics(); renderPublicName(); renderConnections(); } }
    }
  } catch (error) { if (!(error instanceof NetworkError)) { pageError = errorKey(error); if (!status) render(); } }
  finally { polling = false; }
}
on(window, 'scroll', () => { document.documentElement.style.setProperty('--collapse', String(Math.min(1, window.scrollY / 64))); });
on(document, 'visibilitychange', () => { if (document.hidden) videoStats.reset(); syncVisibleTimer(); if (!document.hidden) void tick(); });
on(document, 'keydown', event => {
  if (!['ArrowUp', 'ArrowDown', 'ArrowLeft', 'ArrowRight'].includes(event.key) || event.altKey || event.ctrlKey || event.metaKey) return;
  const target = event.target as HTMLElement;
  if (target.matches('select, textarea') || target.isContentEditable) return;
  if (target.matches('input') && ['ArrowLeft', 'ArrowRight'].includes(event.key)) return;
  const scope = [...document.querySelectorAll('dialog[open]')].at(-1) ?? document;
  const items = [...scope.querySelectorAll<HTMLElement>('button:not(:disabled), a[href], summary, input, select, [tabindex="0"]')].filter(el => el.getClientRects().length > 0);
  const index = items.indexOf(target); const direction = ['ArrowUp', 'ArrowLeft'].includes(event.key) ? -1 : 1;
  const next = items[Math.max(0, Math.min(items.length - 1, index + direction))];
  if (next) { event.preventDefault(); next.focus(); }
});
enhanceSheets(() => t('ui.close'));
render(); void tick(); setInterval(() => void tick(), 2000);
