// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-27

import { updates, releaseMarkup, updateFailureText } from './update-check';
import { qrSvg } from './qr';
import { api, ApiError } from './api';
import { esc } from './html';
import { patchMarkup, on } from './dom';
import type { Translate } from './stage';
export interface LogExport { id: string; bytes: number; sha256: string; createdMillis: number }
export interface LogReceipt { logId: string; trackingUrl: string; qrCodeUrl: string; expiresMillis: number }
export interface ConfigImport { applied: string[]; ignored: string[]; restartNeeded: boolean; appliedNow?: string[]; nextStart?: string[] }
export const CONFIG_LIMIT = 64 * 1024;
export function parseConfig(text: string): unknown {
  if (new TextEncoder().encode(text).length > CONFIG_LIMIT) throw new Error('files.tooLarge');
  let envelope: unknown;
  try { envelope = JSON.parse(text); } catch { throw new Error('files.notJson'); }
  if (!envelope || typeof envelope !== 'object' || Array.isArray(envelope)) throw new Error('files.notJson');
  return envelope;
}
export function configText(bytes: ArrayBuffer): string {
  if (bytes.byteLength > CONFIG_LIMIT) throw new Error('files.tooLarge');
  try { const text = new TextDecoder('utf-8', { fatal: true, ignoreBOM: true }).decode(bytes); parseConfig(text); return text; }
  catch (error) { if (error instanceof Error && error.message.startsWith('files.')) throw error; throw new Error('files.notJson'); }
}
export function configErrorText(error: unknown, t: Translate): string | null {
  if (!(error instanceof ApiError) || !Object.keys(error.errors).length) return null;
  return Object.entries(error.errors).map(([key, code]) => {
    const message = ({ 'too-large': 'files.tooLarge', 'bad-version': 'files.badVersion', 'not-json': 'files.notJson', io: 'files.configIo' } as Record<string, string>)[code];
    return message ? t(message) : t('files.invalid', { key }) + (code.startsWith('error.') ? ` · ${t(code)}` : code === 'conflict' ? ` · ${t('files.conflict')}` : code === 'unsupported' ? ` · ${t('files.unsupported')}` : '');
  }).join('\n');
}
export function httpUrl(value: string): string { try { const url = new URL(value, location.origin); return ['http:', 'https:'].includes(url.protocol) ? url.href : ''; } catch { return ''; } }
export function privacyMarkup(t: Translate): string {
  return ['purpose', 'local', 'server', 'link', 'content', 'retention', 'alternative'].map(key => `<p>${esc(t(`logs_privacy_${key}`))}</p>`).join('');
}
export function mountFeaturePage(container: HTMLElement, route: string, options: { t: Translate; version: string; platform?: 'linux' | 'android'; adminFeatures?: string[]; language: string; setLanguage: (language: string) => void; error: (error: unknown) => string; reload: () => Promise<void> }) {
  const t = options.t;
  let privateDataAvailable = false;
  const feedback = (key: string) => { const node = container.querySelector('#feature-feedback'); if (node) node.textContent = t(key); };
  const fail = (e: unknown) => {
    const message = configErrorText(e, t), node = container.querySelector('#feature-feedback');
    if (message && node) { node.textContent = message; return; }
    feedback(e instanceof ApiError && ['offline', 'too_large', 'rejected', 'timeout'].includes(e.key) ? `files.${e.key}` : e instanceof Error && e.message.startsWith('files.') ? e.message : options.error(e));
  };
  const date = (millis: number) => new Date(millis).toLocaleString();
  const busy = (value: boolean, key = '') => { container.querySelectorAll<HTMLButtonElement>('button').forEach(button => { button.disabled = value || (button.id === 'privacy-export' && !privateDataAvailable); }); container.querySelector('progress')?.toggleAttribute('hidden', !value); container.setAttribute('aria-busy', String(value)); if (key) feedback(key); };
  if (['settings/logs', 'settings/config-files'].includes(route) && !options.adminFeatures?.includes(route.slice(9))) return;
  if (route === 'settings/logs') {
    container.innerHTML = `<div class="inset-group"><div class="chips" role="group" aria-label="${esc(t('logs_choose_period'))}">${['10m', '30m', '24h'].map((range, index) => `<button data-range="${range}" aria-pressed="${index === 0}">${esc(t(['logs_10_minutes', 'logs_30_minutes', 'logs_24_hours'][index]))}</button>`).join('')}</div><button class="pill full-width" id="logs-save">${esc(t('files.savePhone'))}</button><button class="primary pill" id="logs-upload">${esc(t('files.uploadQr'))}</button><progress hidden aria-label="${esc(t('ui.loading'))}"></progress><p id="feature-feedback" class="config-feedback" role="status"></p></div><div id="log-receipt"></div><div class="inset-group privacy"><button class="inset-row full-width" id="logs-privacy">${esc(t('logs_privacy_title'))}<span aria-hidden="true">›</span></button></div>`;
    on(container.querySelector('#logs-privacy'), 'click', () => {
      const dialog = document.createElement('dialog'); dialog.setAttribute('aria-label', t('logs_privacy_title'));
      patchMarkup(dialog, `<h2>${esc(t('logs_privacy_title'))}</h2>${privacyMarkup(t)}<button class="quiet full-width" data-close>${esc(t('ui.close'))}</button>`);
      document.body.append(dialog); dialog.showModal();
      const close = () => { dialog.close(); dialog.remove(); container.querySelector<HTMLElement>('#logs-privacy')?.focus(); };
      on(dialog, 'cancel', close); on(dialog.querySelector('[data-close]'), 'click', close);
    });
    let range: '10m' | '30m' | '24h' = '10m';
    let archive: LogExport | null = null;
    function receiptView(receipt: LogReceipt) {
      if (!container.isConnected) return;
      const tracking = httpUrl(receipt.trackingUrl), qr = httpUrl(receipt.qrCodeUrl);
      const localQr = tracking ? qrSvg(tracking, t('logs_qr_description')) : null;
      patchMarkup(container.querySelector('#log-receipt')!, `<details class="inset-group" open data-key="receipt"><summary>${esc(t('logs_latest_receipt'))}</summary>${localQr ?? (qr ? `<img class="receipt-qr" src="${esc(qr)}" alt="${esc(t('logs_qr_description'))}" referrerpolicy="no-referrer">` : '')}<p>${esc(t('logs_receipt_id', { arg1: receipt.logId }))}</p><p>${esc(t('logs_receipt_expires', { arg1: date(receipt.expiresMillis) }))}</p><a href="${esc(tracking)}" target="_blank" rel="noopener noreferrer">${esc(tracking)}</a><p class="muted">${esc(t('files.receiptFallback'))}</p></details>`);
    }
    void api.logReceipt().then(receiptView).catch(e => { if (!(e instanceof ApiError && e.status === 404)) fail(e); });
    container.querySelectorAll<HTMLButtonElement>('[data-range]').forEach(button => on(button, 'click', () => {
      range = button.dataset.range as typeof range; archive = null;
      container.querySelectorAll('[data-range]').forEach(b => b.setAttribute('aria-pressed', String(b === button)));
    }));
    const exportLogs = async () => { busy(true, 'logs_compressing'); archive = await api.logExport(range); return archive; };
    const save = async () => { try { const result = await exportLogs(); const a = document.createElement('a'); a.href = `/api/v0/logs/export/${encodeURIComponent(result.id)}`; a.download = ''; a.click(); feedback('files.downloadStarted'); } catch (e) { fail(e); } finally { busy(false); } };
    on(container.querySelector('#logs-save'), 'click', () => void save());
    on(container.querySelector('#logs-upload'), 'click', () => {
      const dialog = document.createElement('dialog'); dialog.setAttribute('aria-label', t('logs_consent_title'));
      dialog.innerHTML = `<h2>${esc(t('logs_consent_title'))}</h2>${['logs_consent_destination', 'logs_privacy_server', 'logs_consent_retention', 'logs_consent_alternative'].map(k => `<p>${esc(t(k))}</p>`).join('')}<button data-consent class="primary">${esc(t('logs_consent_agree'))}</button><button data-close>${esc(t('ui.cancel'))}</button>`;
      document.body.append(dialog); dialog.showModal();
      const close = () => { dialog.close(); dialog.remove(); container.querySelector<HTMLButtonElement>('#logs-upload')?.focus(); };
      on(dialog.querySelector('[data-close]'), 'click', close); on(dialog, 'cancel', close);
      on(dialog.querySelector('[data-consent]'), 'click', () => { close(); void (async () => {
        try { const file = archive ?? await exportLogs(); busy(true, 'logs_uploading'); receiptView(await api.logUpload(file.id)); feedback('logs_ready'); }
        catch (e) { fail(e); } finally { busy(false); }
      })(); });
    });
  } else if (route === 'settings/config-files') {
    container.innerHTML = `<div class="inset-group"><a class="inset-row" href="/api/v0/config/user" download="openxplay-user-config.json">${esc(t('files.export.user'))} ↓</a><button class="inset-row full-width" id="privacy-export" disabled>${esc(t('files.export.privacy'))} ↓</button><p id="privacy-empty" class="muted" hidden>${esc(t('files.privacyEmpty'))}</p><button class="pill full-width" id="choose-config">${esc(t('files.chooseFile'))}</button><input hidden id="config-file" aria-label="${esc(t('files.import'))}" type="file" accept="application/json,.json"><p class="muted">${esc(t('files.importHint'))}</p><progress hidden aria-label="${esc(t('ui.loading'))}"></progress><p id="feature-feedback" class="config-feedback" role="status"></p></div><div id="config-preview"></div>`;
    const refreshPrivacy = async () => {
      const settings = await api.settings();
      if (!container.isConnected) return;
      const available = privateDataAvailable = settings.values.privacyConfigAvailable === true;
      container.querySelector<HTMLButtonElement>('#privacy-export')!.disabled = !available;
      container.querySelector('#privacy-empty')!.toggleAttribute('hidden', available);
    };
    void refreshPrivacy().catch(fail);
    on(container.querySelector('#privacy-export'), 'click', () => {
      const dialog = document.createElement('dialog'); dialog.setAttribute('aria-label', t('files.export.privacy'));
      dialog.innerHTML = `<h2>${esc(t('files.export.privacy'))}</h2><p>${esc(t('files.privacyWarning'))}</p><button data-export class="primary">${esc(t('files.exportConfirm'))}</button><button data-close>${esc(t('ui.cancel'))}</button>`;
      document.body.append(dialog); dialog.showModal();
      const close = () => { dialog.close(); dialog.remove(); container.querySelector<HTMLButtonElement>('#privacy-export')?.focus(); };
      on(dialog.querySelector('[data-close]'), 'click', close); on(dialog, 'cancel', close);
      on(dialog.querySelector('[data-export]'), 'click', () => {
        close(); const link = document.createElement('a'); link.href = '/api/v0/config/privacy'; link.download = 'openxplay-privacy-config.json'; link.click();
      });
    });
    on(container.querySelector('#choose-config'), 'click', () => container.querySelector<HTMLInputElement>('#config-file')?.click());
    let envelope: unknown;
    on(container.querySelector<HTMLInputElement>('#config-file'), 'change', async event => {
      const file = (event.target as HTMLInputElement).files?.[0]; if (!file) return;
      patchMarkup(container.querySelector('#config-preview')!, ''); envelope = undefined;
      try {
        if (file.size > CONFIG_LIMIT) throw new Error('files.tooLarge');
        envelope = configText(await file.arrayBuffer()); busy(true, 'files.previewing');
        const preview = await api.configImport(envelope, true);
        if (!container.isConnected) return;
        patchMarkup(container.querySelector('#config-preview')!, `<div class="inset-group"><h2>${esc(t('files.preview'))}</h2><p>${esc(t('files.applied'))}: ${esc(preview.applied.join(', ') || '—')}</p><p>${esc(t('files.ignored'))}: ${esc(preview.ignored.join(', ') || '—')}</p><p>${esc(t('files.appliesNow'))}: ${esc(preview.appliedNow?.join(', ') || '—')}</p><p>${esc(t('files.nextStart'))}: ${esc(preview.nextStart?.join(', ') || '—')}</p>${preview.restartNeeded ? `<p class="notice">${esc(t('files.restart'))}</p>` : ''}<button class="primary" id="config-apply">${esc(t('files.apply'))}</button></div>`);
        on(container.querySelector('#config-apply'), 'click', async () => {
          if (envelope === undefined) return;
          try { busy(true, 'ui.saving'); const result = await api.configImport(envelope); feedback((result as { restartNeeded: boolean }).restartNeeded ? 'files.restart' : 'ui.saved'); envelope = undefined; patchMarkup(container.querySelector('#config-preview')!, ''); await options.reload(); await refreshPrivacy(); }
          catch (e) { fail(e); } finally { busy(false); }
        });
      } catch (e) { fail(e); } finally { busy(false); }
    });
  } else if (route === 'settings/about') {
    const locale = options.language === 'auto' ? (t('files.stable') === 'Stable' ? 'en' : 'zh-CN') : options.language;
    container.innerHTML = `<div class="inset-group"><div class="inset-row"><strong>OpenXPlay Relay</strong><span>${esc(options.version)}</span></div><p>${esc(t('files.project'))}</p><a class="inset-row" href="${locale.startsWith('zh') ? 'https://openxplay.org' : 'https://openxplay.org/en/'}" target="_blank" rel="noopener noreferrer">${esc(t('files.poweredBy'))}</a><a class="inset-row" href="https://github.com/OpenXPlay/OpenXPlay-Relay" target="_blank" rel="noopener noreferrer">${esc(t('files.source'))}</a><a class="inset-row" href="https://github.com/OpenXPlay/OpenXPlay-Relay/blob/main/NOTICE" target="_blank" rel="noopener noreferrer">${esc(t('files.licence'))}</a><label for="language">${esc(t('ui.language'))}</label><select id="language"><option value="auto">${esc(t('ui.automatic'))}</option><option value="en">English</option><option value="zh-CN">简体中文</option></select></div>${options.adminFeatures?.includes('update-check') ? `<div class="inset-group"><label for="update-channel">${esc(t('files.channel'))}</label><select id="update-channel"><option value="stable">${esc(t('files.stable'))}</option><option value="beta">${esc(t('files.beta'))}</option></select><button id="update-check">${esc(t('files.checkUpdate'))}</button><progress hidden aria-label="${esc(t('files.checking'))}"></progress><p id="feature-feedback" role="status"></p><div id="update-result"></div><p class="muted">${esc(t('files.channelHint'))}</p><p class="muted">${esc(t(options.platform === 'linux' ? 'files.updateHint.linux' : 'files.updateHint.android'))}</p></div>` : ''}<div class="inset-group"><h2>${esc(t('files.acknowledgements'))}</h2><p>${esc(t('files.acknowledgementText'))}</p><a class="inset-row" href="https://github.com/shihabal3amri/DiPlay" target="_blank" rel="noopener noreferrer">DiPlay · ${esc(t('files.diplayHint'))}</a><a class="inset-row" href="https://github.com/shilapi/xcertplay" target="_blank" rel="noopener noreferrer">xcertplay · ${esc(t('files.xcertplayHint'))}</a><p>${esc(t('files.trademark'))}</p></div>`;
    container.querySelector<HTMLSelectElement>('#language')!.value = options.language;
    on(container.querySelector('#language'), 'change', e => options.setLanguage((e.target as HTMLSelectElement).value));
    if (!options.adminFeatures?.includes('update-check')) return;
    const refresh = () => {
      container.querySelector<HTMLSelectElement>('#update-channel')!.value = updates.channel;
      const button = container.querySelector<HTMLButtonElement>('#update-check')!;
      button.disabled = updates.checking; button.textContent = t(updates.checking ? 'files.checking' : 'files.checkUpdate');
      container.querySelector('progress')?.toggleAttribute('hidden', !updates.checking);
      const node = container.querySelector('#feature-feedback');
      if (node) node.textContent = updates.failure ? updateFailureText(updates.failure, t, locale) : '';
      patchMarkup(container.querySelector('#update-result')!, updates.result ? releaseMarkup(updates.result, t, locale) : '');
    };
    on(container.querySelector('#update-check'), 'click', () => void updates.check());
    on(container.querySelector('#update-channel'), 'change', event => updates.setChannel((event.target as HTMLSelectElement).value as 'stable' | 'beta'));
    refresh(); return updates.subscribe(refresh);
  }
}
