// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-27

import { openSurface, closeSurface } from './dom';
import { request } from './api';
import { esc } from './html';
import { readIdentitySelection } from './identity-archive';
export interface IdentityStatus {
  mfi: { state: 'READY' | 'MISSING' | 'INVALID'; fingerprint: string | null };
  tls: { state: 'READY' | 'MISSING' | 'INVALID'; fingerprint: string | null; imported: boolean; pending?: boolean };
}
export interface ImportResult { ok: true; followUp?: string; recognized?: 'key' | 'certificate' | 'unknown'; fingerprint?: string }
export const identityApi = {
  status: () => request<IdentityStatus>('/identity', 'GET', undefined, false),
  select: (files: string[], followUp?: string) => request<ImportResult>('/identity/import', 'POST', { files, ...(followUp ? { followUp } : {}) }, false, 60000),
  cancel: (followUp: string) => request('/identity/import', 'POST', { followUp, cancel: true }, false),
  remove: (kind: 'identity' | 'tls') => request<IdentityStatus>('/' + kind, 'DELETE', { confirm: true }, false),
  pem: (chainPem: string, keyPem: string) => request<IdentityStatus>('/tls/import', 'POST', { chainPem, keyPem }, false, 30000),
  tls: (data: string, password: string) => request<IdentityStatus>('/tls/import', 'POST', { data, password }, false, 30000),
};
export const materialKey = (state: string) => state === 'READY' ? 'direlay_installed' : state === 'INVALID' ? 'dev_invalid_identity' : 'direlay_missing';
export class IdentitySelection {
  followUp: string | undefined;
  recognized = 'unknown';
  accept(result: ImportResult) { this.followUp = result.followUp; this.recognized = result.recognized ?? 'unknown'; }
  clear() { this.followUp = undefined; this.recognized = 'unknown'; }
}
export function identityMarkup(status: IdentityStatus | null, selection: IdentitySelection, busy: boolean, message: string, t: (key: string) => string): string {
  const fingerprint = (value?: string | null) => value ? `<p class="selectable fingerprint">${esc(value)}</p><button data-copy="${esc(value)}">${t('identity_copy_fingerprint')}</button>` : `<p>${t('direlay_missing')}</p>`;
  return `<section class="inset-group"><h2>${t('direlay_tls')}</h2><p>${t('dev_fingerprint')}</p>${fingerprint(status?.tls.fingerprint)}
    ${status?.tls.pending ? `<p role="status">${t('identity_tls_pending')}</p>` : ''}
    <label>${t('dev_import_tls')}<input id="tls-file" type="file" accept=".p12,.pfx,application/x-pkcs12" ${busy ? 'disabled' : ''}></label>
    <label>${t('direlay_tls_password')}<input id="tls-password" type="password" maxlength="4096" autocomplete="off" ${busy ? 'disabled' : ''}></label>
    <button id="tls-import" ${busy ? 'disabled' : ''}>${t('dev_import_tls')}</button>
    <details><summary>${t('identity_import_pem')}</summary>
    <label>${t('identity_pem_chain')}<input id="tls-pem-chain" type="file" accept=".pem,.crt,.cer" ${busy ? 'disabled' : ''}></label>
    <label>${t('identity_pem_key')}<input id="tls-pem-key" type="file" accept=".pem,.key" ${busy ? 'disabled' : ''}></label>
    <button id="tls-pem-import" ${busy ? 'disabled' : ''}>${t('identity_import_pem')}</button></details>
    <button id="tls-remove" ${busy || !status?.tls.imported ? 'disabled' : ''}>${t('dev_remove_tls')}</button><p class="muted">${t('dev_tls_hint')}</p></section>
    <details class="inset-group" ${selection.followUp ? 'open' : ''}><summary>${t('direlay_identity')} · ${status ? t(materialKey(status.mfi.state)) : ''}</summary>
    <p>${t('direlay_identity_hint')}</p><p>${t('dev_fingerprint')}</p>${fingerprint(status?.mfi.fingerprint)}
    ${selection.followUp ? `<p role="status">${t('identity_follow_' + selection.recognized)}</p>` : ''}
    <label>${t(selection.followUp ? 'identity_choose_other' : 'direlay_manage_identity')}<input id="identity-files" type="file" ${selection.followUp ? '' : 'multiple'} ${busy ? 'disabled' : ''}></label>
    ${selection.followUp ? `<button id="identity-cancel" ${busy ? 'disabled' : ''}>${t('ui.cancel')}</button>` : ''}
    <button id="identity-remove" ${busy || !status || status.mfi.state === 'MISSING' ? 'disabled' : ''}>${t('dev_remove_identity')}</button></details>
    <p role="${message ? 'alert' : 'status'}">${esc(message ? t(message) : busy ? t('identity_working') : '')}</p>`;
}
export async function fileBase64(file: File, max: number): Promise<string> {
  if (!file.size || file.size > max) throw new Error('identity_error_size');
  const bytes = new Uint8Array(await file.arrayBuffer());
  try {
    let binary = '';
    for (let offset = 0; offset < bytes.length; offset += 8192) binary += String.fromCharCode(...bytes.subarray(offset, offset + 8192));
    return btoa(binary);
  } finally { bytes.fill(0); }
}
export async function filePem(file: File, max: number): Promise<string> {
  if (!file.size || file.size > max) throw new Error('identity_error_pem');
  const bytes = new Uint8Array(await file.arrayBuffer());
  try { return new TextDecoder('utf-8', { fatal: true }).decode(bytes); }
  catch { throw new Error('identity_error_pem'); }
  finally { bytes.fill(0); }
}
export function mountIdentity(container: HTMLElement, selection: IdentitySelection, t: (key: string) => string, error: (e: unknown) => string, reload: () => Promise<void>): () => void {
  let status: IdentityStatus | null = null, busy = false, message = '', disposed = false;
  const show = () => {
    if (disposed) return;
    container.innerHTML = identityMarkup(status, selection, busy, message, t);
    container.querySelectorAll<HTMLButtonElement>('[data-copy]').forEach(button => button.addEventListener('click', () => void navigator.clipboard.writeText(button.dataset.copy!)));
    container.querySelector('#identity-files')?.addEventListener('change', event => {
      const input = event.target as HTMLInputElement, files = [...(input.files ?? [])]; input.value = '';
      if (!files.length) return;
      void run(async () => {
        const encoded = await readIdentitySelection(files, !!selection.followUp);
        selection.accept(await identityApi.select(encoded, selection.followUp));
      });
    });
    container.querySelector('#identity-cancel')?.addEventListener('click', () => void run(async () => { if (selection.followUp) await identityApi.cancel(selection.followUp); selection.clear(); }));
    container.querySelector('#tls-import')?.addEventListener('click', () => {
      const file = (container.querySelector('#tls-file') as HTMLInputElement).files?.[0];
      const field = container.querySelector('#tls-password') as HTMLInputElement;
      const password = field.value; field.value = '';
      if (!file) { message = 'identity_error_tls'; show(); return; }
      void run(async () => { status = await identityApi.tls(await fileBase64(file, 1024 * 1024), password); });
    });
    container.querySelector('#tls-pem-import')?.addEventListener('click', () => {
      const chain = (container.querySelector('#tls-pem-chain') as HTMLInputElement).files?.[0];
      const key = (container.querySelector('#tls-pem-key') as HTMLInputElement).files?.[0];
      if (!chain || !key) { message = 'identity_error_pem'; show(); return; }
      void run(async () => { status = await identityApi.pem(await filePem(chain, 2 * 1024 * 1024), await filePem(key, 1024 * 1024)); });
    });
    for (const kind of ['identity', 'tls'] as const) container.querySelector('#' + kind + '-remove')?.addEventListener('click', () => confirmRemove(kind));
  };
  const run = async (work: () => Promise<void>) => {
    if (busy) return;
    busy = true; message = ''; show();
    try { await work(); status = await identityApi.status(); await reload(); }
    catch (e) { message = e instanceof Error && e.message.startsWith('identity_') ? e.message : error(e); }
    finally { busy = false; show(); }
  };
  let dialog: HTMLDialogElement | null = null;
  const confirmRemove = (kind: 'identity' | 'tls') => {
    if (dialog) closeSurface(dialog); dialog = document.createElement('dialog');
    dialog.innerHTML = `<h2>${t(kind === 'identity' ? 'dev_remove_identity_question' : 'dev_remove_tls_question')}</h2><div class="dialog-actions"><button class="cancel">${t('ui.cancel')}</button><button class="confirm">${t('direlay_remove')}</button></div>`;
    const close = () => { if (dialog) closeSurface(dialog); dialog = null; };
    dialog.querySelector('.cancel')!.addEventListener('click', close);
    dialog.addEventListener('cancel', close);
    dialog.querySelector('.confirm')!.addEventListener('click', () => { close(); void run(async () => { status = await identityApi.remove(kind); if (kind === 'identity') selection.clear(); }); });
    document.body.append(dialog); openSurface(dialog);
  };
  show(); void run(async () => { status = await identityApi.status(); });
  return () => { disposed = true; if (dialog) closeSurface(dialog); };
}
