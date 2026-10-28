// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-27

import { afterEach, expect, it, vi } from 'vitest';
import { api, ApiError } from '../src/api';
import { translate } from '../src/i18n';
import { uploadProgress, uploadFailure, receiptMarkup, privacyMarkup, type LogJob } from '../src/logs-page';
import { qrSvg } from '../src/qr';
vi.mock('../src/qr', () => ({ qrSvg: vi.fn(() => '<svg></svg>') }));
vi.mock('../src/dom', async () => ({ ...(await vi.importActual('../src/dom')), patchMarkup: (target: Host, markup: string) => { target.innerHTML = markup; } }));
class Host extends EventTarget {
  innerHTML = ''; textContent = ''; disabled = false; hidden = false; isConnected = true; href = ''; download = '';
  dataset: Record<string, string> = {}; children = new Map<string, Host>();
  querySelector(selector: string): Host { if (!this.children.has(selector)) this.children.set(selector, new Host()); return this.children.get(selector)!; }
  querySelectorAll(): Host[] { return []; }
  toggleAttribute(name: string, value: boolean) { if (name === 'hidden') this.hidden = value; }
  setAttribute() {} showModal() {} close() {} remove() {} focus() {} click() { this.dispatchEvent(new Event('click')); }
}
const job: LogJob = { id: 'job', exportId: 'zip', state: 'running', phase: 'Sending', sent: 1024, total: 2048, elapsedMillis: 15000, receipt: null, error: null };
afterEach(() => { vi.restoreAllMocks(); vi.unstubAllGlobals(); vi.useRealTimers(); });
it.each(['en', 'zh-CN'] as const)('shows phases, bytes, elapsed time and the 15 s hint (%s)', language => {
  const t = (key: string, args?: Record<string, string | number>) => translate(language, key, args);
  const sending = uploadProgress(t, job, true);
  expect(sending).toContain('50%'); expect(sending).toContain('1,024 B'); expect(sending).toContain('0:15');
  expect(sending).toContain(t('logs_upload_wait.linux'));
  expect(uploadProgress(t, { ...job, elapsedMillis: 14999 }, true)).not.toContain(t('logs_upload_wait.linux'));
  expect(uploadProgress(t, { ...job, phase: 'Connecting' }, true)).toContain(t('logs_connecting'));
  expect(uploadProgress(t, { ...job, phase: 'AwaitingReceipt', sent: 2048 }, true)).toContain(t('logs_confirming_detail'));
  expect(sending).not.toContain(t('logs_ready'));
  const failure = uploadFailure(t, { code: 'http', status: 429, retryAfterSeconds: 45 });
  expect(failure).toContain(t('logs_upload_http', { arg1: 429 })); expect(failure).toContain(t('logs_retry_after', { arg1: 45 }));
});
it('retains the confirmed link and Copy action if QR creation throws', () => {
  vi.stubGlobal('location', { origin: 'https://relay.local' });
  vi.mocked(qrSvg).mockImplementationOnce(() => { throw new Error('QR failed'); });
  const html = receiptMarkup(key => translate('en', key), { logId: 'confirmed', trackingUrl: 'https://logs.openxplay.org/r/confirmed', qrCodeUrl: '/unused.png', expiresMillis: Date.now() + 10000 });
  expect(html).toContain('confirmed'); expect(html).toContain('logs-copy'); expect(html).toContain(translate('en', 'logs_qr_unavailable')); expect(html).not.toContain('<img');
});
it.each(['en', 'zh-CN'] as const)('describes Linux journal/export privacy precisely (%s)', language => {
  const text = privacyMarkup(key => translate(language, key), 'linux');
  expect(text).toContain(language === 'en' ? 'system journal' : '系统日志');
  expect(text).toContain(language === 'en' ? 'over the network' : '通过网络');
  expect(text).not.toMatch(/this phone|Android version|24 hours|before anything is stored|这部手机|约 24 小时/);
});
it('cancels a job, refreshes the last receipt and saves the same retained ZIP', async () => {
  vi.useFakeTimers();
  const dialogs: Host[] = [], downloads: string[] = [];
  vi.stubGlobal('document', { body: { append: (node: Host) => dialogs.push(node) }, createElement: (tag: string) => { const node = new Host(); if (tag === 'a') node.click = () => { downloads.push(node.href); }; return node; } });
  vi.spyOn(api, 'logReceipt').mockRejectedValue(new ApiError(404, 'error.request'));
  const exportCall = vi.spyOn(api, 'logExport').mockResolvedValue({ id: 'retained-zip', bytes: 2048, sha256: 'hash', createdMillis: Date.now() });
  const upload = vi.spyOn(api, 'logUpload').mockResolvedValue(job);
  const status = vi.spyOn(api, 'logUploadStatus').mockResolvedValue(job);
  const cancel = vi.spyOn(api, 'logUploadCancel').mockResolvedValue({ ...job, state: 'cancelled' });
  const { mountLogsPage } = await import('../src/logs-page');
  const container = new Host();
  mountLogsPage(container as unknown as HTMLElement, { t: (key, args) => translate('en', key, args), platform: 'linux', error: () => 'error.request' });
  container.querySelector('#logs-upload').click(); expect(upload).not.toHaveBeenCalled();
  dialogs[0].querySelector('[data-consent]').click();
  await vi.advanceTimersByTimeAsync(0); expect(upload).toHaveBeenCalledOnce();
  status.mockResolvedValue({ ...job, state: 'cancelled' });
  container.querySelector('#logs-cancel').click(); await vi.advanceTimersByTimeAsync(0);
  expect(cancel).toHaveBeenCalledWith('job'); expect(api.logReceipt).toHaveBeenCalledTimes(3);
  expect(container.querySelector('#feature-feedback').textContent).toBe(translate('en', 'logs_upload_cancelled'));
  container.querySelector('#logs-save').click(); await vi.advanceTimersByTimeAsync(0);
  expect(exportCall).toHaveBeenCalledOnce(); expect(downloads).toEqual(['/api/v0/logs/export/retained-zip']);
});
it('does not retry upload start or cancel requests after a lost response', async () => {
  vi.stubGlobal('fetch', vi.fn().mockRejectedValue(new TypeError('offline')));
  await expect(api.logUpload('zip')).rejects.toThrow(); expect(fetch).toHaveBeenCalledOnce();
  vi.mocked(fetch).mockClear(); await expect(api.logUploadCancel('job')).rejects.toThrow(); expect(fetch).toHaveBeenCalledOnce();
});
it('explains an empty period without offering a metadata-only ZIP for download or upload', async () => {
  vi.useFakeTimers(); vi.setSystemTime(Date.now() + 7200000);
  const downloads: Host[] = [];
  vi.stubGlobal('document', { body: { append: () => {} }, createElement: () => { const node = new Host(); node.click = () => { downloads.push(node); }; return node; } });
  vi.spyOn(api, 'logReceipt').mockRejectedValue(new ApiError(404, 'error.request'));
  vi.spyOn(api, 'logExport').mockResolvedValue({ id: 'empty', bytes: 10, sha256: 'hash', createdMillis: Date.now(), empty: true, firstMillis: null, lastMillis: null });
  const upload = vi.spyOn(api, 'logUpload');
  const { mountLogsPage } = await import('../src/logs-page'); const container = new Host();
  mountLogsPage(container as unknown as HTMLElement, { t: (key, args) => translate('en', key, args), platform: 'linux', error: () => 'error.request' });
  container.querySelector('#logs-save').click(); await vi.advanceTimersByTimeAsync(0);
  expect(container.querySelector('#feature-feedback').textContent).toBe(translate('en', 'logs_empty'));
  expect(downloads).toHaveLength(0); expect(upload).not.toHaveBeenCalled();
});
