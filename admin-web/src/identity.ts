// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-27

import './stage.css';
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
export function identityMarkup(status: IdentityStatus | null, selection: IdentitySelection, busy: boolean, message: string, t: (key: string) => string, fileNames: Record<string, string> = {}): string {
  const fingerprint = (value?: string | null) => value ? `<p class="selectable fingerprint">${esc(value)}</p><button data-copy="${esc(value)}">${t('identity_copy_fingerprint')}</button>` : '';
  const picker = (id: string, label: string, attributes = '') => `<label class="identity-file"><span>${esc(t(label))}</span><input id="${id}" type="file" ${attributes} ${busy ? 'disabled' : ''}><span class="button pill" aria-hidden="true">${esc(t('identity_choose_file'))}</span><span class="identity-file-name" data-file-name="${id}">${esc(fileNames[id] ?? t('identity_no_file'))}</span></label>`;
  const tls = `<details class="inset-group" id="identity-tls"><summary>${t('direlay_tls')}</summary><p>${t('dev_fingerprint')}</p>${fingerprint(status?.tls.fingerprint)}
    ${status?.tls.pending ? `<p role="status">${t('identity_tls_pending')}</p>` : ''}
    ${picker('tls-file', 'dev_import_tls', 'accept=".p12,.pfx,application/x-pkcs12"')}
    <label>${t('direlay_tls_password')}<input id="tls-password" type="password" maxlength="4096" autocomplete="off" ${busy ? 'disabled' : ''}></label>
    <button id="tls-import" ${busy ? 'disabled' : ''}>${t('dev_import_tls')}</button>
    <details id="identity-pem"><summary>${t('identity_import_pem')}</summary>
    ${picker('tls-pem-chain', 'identity_pem_chain', 'accept=".pem,.crt,.cer"')}
    ${picker('tls-pem-key', 'identity_pem_key', 'accept=".pem,.key"')}
    <button id="tls-pem-import" ${busy ? 'disabled' : ''}>${t('identity_import_pem')}</button></details>
    <button id="tls-remove" ${busy || !status?.tls.imported ? 'disabled' : ''}>${t('dev_remove_tls')}</button>${!status?.tls.imported ? `<p class="muted">${t('identity_tls_automatic')}</p>` : ''}<p class="muted">${t('dev_tls_hint')}</p></details>`;
  return `<details class="inset-group" id="carplay-identity" open><summary>${t('direlay_identity')}</summary>
    <p>${t('direlay_identity_hint')}</p><p>${t('dev_fingerprint')}</p>${fingerprint(status?.mfi.fingerprint)}
    ${selection.followUp ? `<p role="status">${t('identity_follow_' + selection.recognized)}</p>` : ''}
    ${picker('identity-files', selection.followUp ? 'identity_choose_other' : 'direlay_manage_identity', selection.followUp ? '' : 'multiple')}
    ${selection.followUp ? `<button id="identity-cancel" ${busy ? 'disabled' : ''}>${t('ui.cancel')}</button>` : ''}
    <button id="identity-remove" ${busy || !status || status.mfi.state === 'MISSING' ? 'disabled' : ''}>${t('dev_remove_identity')}</button></details>${tls}
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
  let status: IdentityStatus | null = null, busy = false, message = '', disposed = false, focused = false;
  const fileNames: Record<string, string> = {};
  const show = () => {
    if (disposed) return;
    const expanded = ['identity-tls', 'identity-pem'].filter(id => container.querySelector<HTMLDetailsElement>('#' + id)?.open);
    container.innerHTML = identityMarkup(status, selection, busy, message, t, fileNames);
    for (const id of expanded) container.querySelector<HTMLDetailsElement>('#' + id)!.open = true;
    if (!focused && !busy && status) {
      const summary = container.querySelector<HTMLElement>('#carplay-identity summary');
      summary?.scrollIntoView({ block: 'start', behavior: 'instant' }); summary?.focus({ preventScroll: true }); focused = true;
    }
    container.querySelectorAll<HTMLInputElement>('input[type="file"]').forEach(input => input.addEventListener('change', () => {
      fileNames[input.id] = [...(input.files ?? [])].map(file => file.name).join(', ') || t('identity_no_file');
      const line = container.querySelector<HTMLElement>(`[data-file-name="${input.id}"]`); if (line) line.textContent = fileNames[input.id];
    }));
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
    finally { for (const id of Object.keys(fileNames)) delete fileNames[id]; busy = false; show(); }
  };
  let dialog: HTMLDialogElement | null = null;
  const confirmRemove = (kind: 'identity' | 'tls') => {
    dialog?.remove(); dialog = document.createElement('dialog');
    dialog.innerHTML = `<h2>${t(kind === 'identity' ? 'dev_remove_identity_question' : 'dev_remove_tls_question')}</h2><div class="dialog-actions"><button class="cancel">${t('ui.cancel')}</button><button class="confirm">${t('direlay_remove')}</button></div>`;
    const close = () => { dialog?.close(); dialog?.remove(); dialog = null; };
    dialog.querySelector('.cancel')!.addEventListener('click', close);
    dialog.addEventListener('cancel', close);
    dialog.querySelector('.confirm')!.addEventListener('click', () => { close(); void run(async () => { status = await identityApi.remove(kind); if (kind === 'identity') selection.clear(); }); });
    document.body.append(dialog); dialog.showModal();
  };
  show(); void run(async () => { status = await identityApi.status(); });
  return () => { disposed = true; dialog?.remove(); };
}
