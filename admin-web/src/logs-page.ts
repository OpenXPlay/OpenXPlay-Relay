// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-27

import { api, ApiError } from './api';
import { qrSvg } from './qr';
import { esc } from './html';
import { on, patchMarkup } from './dom';
import type { Translate } from './stage';

export interface LogExport { id: string; bytes: number; sha256: string; createdMillis: number; firstMillis?: number | null; lastMillis?: number | null; empty?: boolean; truncated?: boolean }
export interface LogReceipt { logId: string; trackingUrl: string; qrCodeUrl: string; expiresMillis: number }
export interface LogJob {
  id: string; exportId: string; state: 'running' | 'completed' | 'cancelled' | 'failed';
  phase: 'Connecting' | 'Sending' | 'AwaitingReceipt'; sent: number; total: number; elapsedMillis: number;
  receipt: LogReceipt | null; error: { code: string; status: number | null; retryAfterSeconds: number | null } | null;
}
type Range = '10m' | '30m' | '24h';
type Options = { t: Translate; platform?: 'linux' | 'android'; error: (error: unknown) => string };
// Keep recovery available across navigation; the relay bounds retained archives to one hour.
let retained: { range: Range; archive: LogExport; platform?: string } | null = null;
let currentJob: LogJob | null = null;
export function privacyMarkup(t: Translate, platform?: string): string {
  return ['purpose', 'local', 'server', 'link', 'content', 'retention', 'alternative'].map(key => `<p>${esc(t(`logs_privacy_${key}${platform === 'linux' && ['local', 'link', 'content', 'alternative'].includes(key) ? '.linux' : ''}`))}</p>`).join('');
}
export function uploadFailure(t: Translate, error: LogJob['error']): string {
  if (!error) return t('logs_upload_error');
  const key = error.status !== null ? 'logs_upload_http' : error.code === 'offline' ? 'logs_no_internet' : error.code === 'too_large' ? 'logs_too_large' : 'logs_upload_error';
  return t(key, { arg1: error.status ?? 0 }) + (error.retryAfterSeconds !== null ? `\n${t('logs_retry_after', { arg1: error.retryAfterSeconds })}` : '');
}
export function uploadProgress(t: Translate, job: LogJob, linux: boolean): string {
  const seconds = Math.floor(job.elapsedMillis / 1000);
  const phase = job.phase === 'Connecting' ? 'logs_connecting' : job.phase === 'Sending' ? 'logs_uploading' : 'logs_confirming';
  const detail = job.phase === 'Connecting' ? `logs_connecting_detail${linux ? '.linux' : ''}` : job.phase === 'Sending' ? 'logs_sending_detail' : 'logs_confirming_detail';
  const percent = job.total > 0 ? Math.min(100, Math.floor(job.sent * 100 / job.total)) : 0;
  return `<p>${esc(t(phase))}</p>${job.phase === 'Sending' ? `<progress max="${job.total}" value="${job.sent}" aria-label="${esc(t(phase))}"></progress><p>${esc(t('logs_bytes_sent', { arg1: percent, arg2: `${job.sent.toLocaleString()} B`, arg3: `${job.total.toLocaleString()} B` }))}</p>` : `<progress aria-label="${esc(t(phase))}"></progress>`}<p>${esc(t(detail))}</p><p>${esc(t('logs_elapsed', { arg1: Math.floor(seconds / 60), arg2: String(seconds % 60).padStart(2, '0') }))}</p>${seconds >= 15 ? `<p>${esc(t(`logs_upload_wait${linux ? '.linux' : ''}`))}</p>` : ''}`;
}
export function receiptMarkup(t: Translate, receipt: LogReceipt): string {
  let tracking: string;
  try { const url = new URL(receipt.trackingUrl, location.origin); tracking = ['https:', 'http:'].includes(url.protocol) ? url.href : ''; } catch { tracking = ''; }
  let qr: string | null = null;
  try { if (tracking) qr = qrSvg(tracking, t('logs_qr_description')); } catch { /* The confirmed link remains usable. */ }
  return `<details class="inset-group" open data-key="receipt"><summary>${esc(t('logs_latest_receipt'))}</summary>${qr ?? `<p>${esc(t('logs_qr_unavailable'))}</p>`}<p>${esc(t('logs_receipt_id', { arg1: receipt.logId }))}</p><p>${esc(t('logs_receipt_expires', { arg1: new Date(receipt.expiresMillis).toLocaleString() }))}</p>${tracking ? `<a href="${esc(tracking)}" target="_blank" rel="noopener noreferrer">${esc(tracking)}</a><button id="logs-copy">${esc(t('logs_copy'))}</button>` : ''}<p class="muted">${esc(t('logs_scan'))}</p></details>`;
}
export function mountLogsPage(container: HTMLElement, options: Options) {
  const { t } = options, linux = options.platform === 'linux';
  let range: Range = retained && retained.platform === options.platform ? retained.range : '10m';
  let archive: LogExport | null = retained && retained.platform === options.platform ? retained.archive : null;
  if (archive && Date.now() - archive.createdMillis >= 3600000) { archive = null; retained = null; }
  container.innerHTML = `<div class="inset-group"><div class="chips" role="group" aria-label="${esc(t('logs_choose_period'))}">${(['10m', '30m', '24h'] as Range[]).map((value, index) => `<button data-range="${value}" aria-pressed="${value === range}">${esc(t(['logs_10_minutes', 'logs_30_minutes', 'logs_24_hours'][index]))}</button>`).join('')}</div><div id="log-archive"></div><button class="pill full-width" id="logs-save">${esc(t(linux ? 'logs_save.linux' : 'logs_save'))}</button><button class="primary pill" id="logs-upload">${esc(t('logs_upload'))}</button><div id="logs-progress" role="status" aria-live="polite"></div><button id="logs-cancel" hidden>${esc(t('ui.cancel'))}</button><p id="feature-feedback" role="status"></p></div><div id="log-receipt"></div><div class="inset-group privacy"><button class="inset-row full-width" id="logs-privacy">${esc(t('logs_privacy_title'))}<span aria-hidden="true">›</span></button></div>`;
  const anchor = container.querySelector('#logs-progress');
  const active = () => container.isConnected && container.querySelector('#logs-progress') === anchor;
  const feedback = (text: string) => { if (active()) container.querySelector('#feature-feedback')!.textContent = text; };
  const busy = (value: boolean, cancel = false) => {
    container.querySelectorAll<HTMLButtonElement>('[data-range], #logs-save, #logs-upload').forEach(button => { button.disabled = value; });
    container.querySelector('#logs-cancel')!.toggleAttribute('hidden', !cancel);
  };
  const archiveView = () => {
    patchMarkup(container.querySelector('#log-archive')!, archive ? `<p>${esc(t('logs_archive', { arg1: archive.bytes.toLocaleString() }))}</p>${archive.firstMillis != null && archive.lastMillis != null ? `<p>${esc(t('logs_coverage', { arg1: new Date(archive.firstMillis).toLocaleString(), arg2: new Date(archive.lastMillis).toLocaleString() }))}</p>` : ''}${archive.truncated ? `<p>${esc(t('logs_limited'))}</p>` : ''}` : '');
  };
  const receiptView = (receipt: LogReceipt) => {
    if (!active()) return;
    patchMarkup(container.querySelector('#log-receipt')!, receiptMarkup(t, receipt));
    on(container.querySelector('#logs-copy'), 'click', async () => {
      try { await navigator.clipboard.writeText(receipt.trackingUrl); feedback(t('logs_copied')); }
      catch { feedback(t('files.receiptFallback')); }
    });
  };
  const refreshReceipt = async () => { try { receiptView(await api.logReceipt()); } catch (e) { if (!(e instanceof ApiError && e.status === 404)) feedback(t(options.error(e))); } };
  void refreshReceipt(); archiveView();
  on(container.querySelector('#logs-privacy'), 'click', () => {
    const dialog = document.createElement('dialog'); dialog.setAttribute('aria-label', t('logs_privacy_title'));
    dialog.innerHTML = `<h2>${esc(t('logs_privacy_title'))}</h2>${privacyMarkup(t, options.platform)}<button data-close>${esc(t('ui.close'))}</button>`;
    document.body.append(dialog); dialog.showModal();
    const close = () => { dialog.close(); dialog.remove(); container.querySelector<HTMLElement>('#logs-privacy')?.focus(); };
    on(dialog, 'cancel', close); on(dialog.querySelector('[data-close]'), 'click', close);
  });
  container.querySelectorAll<HTMLButtonElement>('[data-range]').forEach(button => on(button, 'click', () => {
    range = button.dataset.range as Range; archive = null; retained = null; archiveView(); feedback('');
    container.querySelectorAll('[data-range]').forEach(b => b.setAttribute('aria-pressed', String(b === button)));
  }));
  const exportLogs = async () => {
    if (archive && Date.now() - archive.createdMillis < 3600000) return archive;
    busy(true); feedback(t('logs_compressing'));
    const result = await api.logExport(range);
    if (!active()) return null;
    if (result.empty) { feedback(t('logs_empty')); return null; }
    archive = result; retained = { range, archive, platform: options.platform }; archiveView(); return archive;
  };
  on(container.querySelector('#logs-save'), 'click', async () => {
    try { const file = await exportLogs(); if (!file || !active()) return; const link = document.createElement('a'); link.href = `/api/v0/logs/export/${encodeURIComponent(file.id)}`; link.download = ''; link.click(); feedback(t('files.downloadStarted')); }
    catch (e) { feedback(t(options.error(e))); } finally { if (active()) busy(false); }
  });
  let polling = false;
  async function poll() {
    if (polling || !active() || !currentJob) return;
    polling = true;
    try {
      const job = await api.logUploadStatus(currentJob.id);
      currentJob = job;
      if (!active()) return;
      if (job.state === 'running') { busy(true, true); patchMarkup(anchor!, uploadProgress(t, job, linux)); }
      else {
        currentJob = null; busy(false); patchMarkup(anchor!, '');
        if (job.receipt) receiptView(job.receipt);
        feedback(job.state === 'cancelled' ? t('logs_upload_cancelled') : job.state === 'failed' ? uploadFailure(t, job.error) : t('logs_ready'));
        await refreshReceipt();
      }
    } catch { feedback(t('logs_upload_error')); /* Poll the same attempt; never repeat a POST. */ }
    finally { polling = false; if (currentJob && active()) setTimeout(() => void poll(), 1000); }
  }
  on(container.querySelector('#logs-cancel'), 'click', async () => {
    if (!currentJob) return;
    try { currentJob = await api.logUploadCancel(currentJob.id); feedback(t('logs_upload_cancelled')); await refreshReceipt(); void poll(); }
    catch { feedback(t('logs_upload_error')); }
  });
  on(container.querySelector('#logs-upload'), 'click', () => {
    const dialog = document.createElement('dialog'); dialog.setAttribute('aria-label', t('logs_consent_title'));
    dialog.innerHTML = `<h2>${esc(t('logs_consent_title'))}</h2>${['logs_consent_destination', 'logs_privacy_server', 'logs_consent_retention', `logs_consent_alternative${linux ? '.linux' : ''}`].map(key => `<p>${esc(t(key))}</p>`).join('')}<button data-consent class="primary">${esc(t('logs_consent_agree'))}</button><button data-close>${esc(t('ui.cancel'))}</button>`;
    document.body.append(dialog); dialog.showModal();
    const close = () => { dialog.close(); dialog.remove(); container.querySelector<HTMLElement>('#logs-upload')?.focus(); };
    on(dialog.querySelector('[data-close]'), 'click', close); on(dialog, 'cancel', close);
    on(dialog.querySelector('[data-consent]'), 'click', () => {
      close(); void (async () => {
        try {
          const file = await exportLogs(); if (!file || !active()) return;
          busy(true); feedback(t('logs_uploading'));
          const result = await api.logUpload(file.id);
          if ('trackingUrl' in result) { receiptView(result); feedback(t('logs_ready')); busy(false); }
          else { currentJob = result; if (active()) { busy(true, true); patchMarkup(anchor!, uploadProgress(t, result, linux)); void poll(); } }
        } catch (e) {
          if (e instanceof ApiError && e.key === 'too_large') feedback(t('logs_too_large'));
          else feedback(t('logs_upload_error'));
          await refreshReceipt(); if (active()) busy(false);
        } finally { if (active() && !currentJob) busy(false); }
      })();
    });
  });
  if (currentJob && linux) { busy(true, true); void poll(); }
}
