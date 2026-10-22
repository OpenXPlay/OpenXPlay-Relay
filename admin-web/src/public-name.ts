// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-20

import type { PublicNameStatus, Status } from './api';
import { api, ApiError, type PublicNameImport } from './api';
import type { PublicNameProviders, Settings, Values } from './schema';
import { publicNameUrl } from './logic';

type Translate = (key: string, parameters?: Record<string, string | number>) => string;
const esc = (s: unknown) => String(s ?? '').replace(/[&<>"']/g, c => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' })[c]!);
export const TERMS_URL = 'https://letsencrypt.org/repository/';
export const PUBLIC_NAME_MAX_BYTES = 16 * 1024;
export type PublicNameMessage = string | PublicNameImport | { errors: Record<string, string> };
interface Draft { name: string; token: string }

/** Tokens typed here stay in memory; untouched providers follow the relay's saved view. */
export class PublicNameDrafts {
  private edits = new Map<string, Draft>();
  fields(provider: string, values: Values) {
    const saved = (values.publicNameProviders as PublicNameProviders | undefined)?.[provider];
    return { ...(this.edits.get(provider) ?? { name: saved?.name ?? '', token: '' }), tokenSet: saved?.tokenSet === true };
  }
  edit(provider: string, values: Values) {
    if (provider) this.edits.set(provider, { name: String(values.publicName ?? ''), token: String(values.publicNameToken ?? '') });
  }
  select(provider: string, values: Values) {
    const draft = this.fields(provider, values);
    Object.assign(values, { publicNameProvider: provider, publicName: draft.name, publicNameToken: draft.token, publicNameTokenSet: draft.tokenSet });
  }
  imported(provider: string, values: Values) {
    this.edits.delete(provider);
    this.select(provider, values);
  }
  saved(submitted: Values, values: Values) {
    const provider = String(submitted.publicNameProvider ?? '');
    if (!provider) return;
    const providers = (values.publicNameProviders ?? {}) as PublicNameProviders;
    values.publicNameProviders = { ...providers, [provider]: { name: String(submitted.publicName).trim().toLowerCase(), tokenSet: !!submitted.publicNameToken || providers[provider]?.tokenSet === true } };
    const draft = this.edits.get(provider);
    if (draft?.name === submitted.publicName && draft.token === submitted.publicNameToken) this.edits.delete(provider);
    else if (draft?.token === submitted.publicNameToken) this.edits.set(provider, { ...draft, token: '' });
    if (values.publicNameProvider === provider) this.select(provider, values);
  }
}
export function publicNameSubmission(values: Values): Values {
  return Object.fromEntries(['publicNameEnabled', 'publicNameProvider', 'publicName', 'publicNameToken', 'publicNameAgreeTerms'].map(key => [key, values[key]]));
}
export function publicNameNameHint(values: Values): string {
  return `publicName.nameHint.${values.publicNameProvider === 'dynv6' ? 'dynv6' : 'duckdns'}`;
}
export function publicNameGuidance(values: Values, t: Translate, language: string): string {
  const provider = values.publicNameProvider;
  if (provider !== 'duckdns' && provider !== 'dynv6') return '';
  const url = provider === 'duckdns' ? 'https://www.duckdns.org/' : 'https://dynv6.com/';
  const guide = language === 'zh-CN' ? 'https://openxplay.org/trusted-https' : 'https://openxplay.org/en/trusted-https';
  return `${esc(t(`publicName.hint.${provider}`))}<br><a href="${url}" target="_blank" rel="noopener">${esc(t(`publicName.open.${provider}`))}</a> · <a href="${guide}" target="_blank" rel="noopener">${esc(t('publicName.guide'))}</a>`;
}
export async function importPublicNameFile(file: Pick<File, 'size' | 'text'>, send = api.publicNameImport): Promise<PublicNameImport> {
  if (file.size > PUBLIC_NAME_MAX_BYTES) throw new ApiError(422, 'error.request', { file: 'too-large' });
  return send(await file.text());
}
export function publicNameMessageLines(message: PublicNameMessage, t: Translate): string[] {
  if (typeof message === 'string') return message ? [t(message)] : [];
  if ('errors' in message) return Object.entries(message.errors).map(([key, code]) => t('publicName.fileError', { key, problem: t(`publicName.file.${code}`) }));
  return [t('publicName.imported', { provider: t(`option.publicNameProvider.${message.provider}`), name: message.name })];
}

/** Trusted HTTPS appears only where the relay's schema has it (Linux; Android configures it in the app). */
export function hasPublicName(settings: Settings): boolean {
  return settings.schema.some(section => section.id === 'publicName');
}
/** Whether `public-name.json` holds anything, so that Remove has something to remove. */
export function publicNameConfigured(values: Values, status?: PublicNameStatus): boolean {
  return values.publicNameEnabled === true || values.publicNameTokenSet === true || !!values.publicName || !!values.publicNameProvider || (!!status && status.state !== 'off');
}
const time = (seconds: number, language: string, withDate: boolean) => new Date(seconds * 1000).toLocaleString(language, withDate ? { dateStyle: 'medium' } : { month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit' });
/** The status line: one of the shared labels, with the head-unit address, dates and error filled in. */
export function publicNameLine(status: Status, t: Translate, language: string): string {
  const p = status.publicName;
  if (!p) return '';
  if (p.state === 'ready') return t('publicName.status.ready', { address: publicNameUrl(status) ?? '', date: p.notAfter ? time(p.notAfter, language, true) : '—' });
  if (p.state === 'error') {
    const problem = t(`publicName.error.${p.error ?? 'provider-unreachable'}`, { detail: p.errorDetail ?? '' });
    return t('publicName.status.error', { problem, time: p.nextAttempt ? time(p.nextAttempt, language, false) : '—' });
  }
  return t(`publicName.status.${p.state}`);
}
/** Problems in `public-name.json` the page cannot fix by itself, one line per key. */
export function publicNameFileErrors(status: Status, t: Translate): string[] {
  return Object.entries(status.publicName?.fileErrors ?? {}).map(([key, code]) => t('publicName.fileError', { key, problem: t(`publicName.file.${code}`) }));
}
/** The status line, the agreement link and the buttons below the section's fields. */
export function publicNamePanel(status: Status, settings: Settings, busy: boolean, message: PublicNameMessage, t: Translate, language: string): string {
  if (!hasPublicName(settings)) return '';
  const values = settings.values;
  const enabled = values.publicNameEnabled === true;
  const tone = status.publicName?.state === 'error' ? 'error' : status.publicName?.state === 'ready' ? 'state-active' : 'muted';
  const files = publicNameFileErrors(status, t);
  return `${enabled ? `<p class="muted"><a href="${TERMS_URL}" target="_blank" rel="noopener">${esc(TERMS_URL)}</a></p>` : ''}<p id="public-name-status" class="${tone}" role="status" aria-live="polite">${esc(publicNameLine(status, t, language))}</p>${files.map(line => `<p class="error">${esc(line)}</p>`).join('')}<div class="public-name-actions"><button id="public-name-import" type="button" ${busy ? 'disabled' : ''}>${t('publicName.import')}</button><input id="public-name-file" type="file" accept=".json" hidden>${enabled && status.publicName?.state === 'error' ? `<button id="public-name-renew" type="button" ${busy ? 'disabled' : ''}>${t('publicName.renew')}</button>` : ''}${publicNameConfigured(values, status.publicName) ? `<button id="public-name-remove" type="button" class="quiet" ${busy ? 'disabled' : ''}>${t('publicName.remove')}</button>` : ''}</div><div id="public-name-result" class="${typeof message === 'object' && 'ok' in message ? 'muted' : 'error'}" role="alert">${publicNameMessageLines(message, t).map(line => `<p>${esc(line)}</p>`).join('')}</div>`;
}
