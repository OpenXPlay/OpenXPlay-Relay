// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-20

import type { PublicNameStatus, Status } from './api';
import type { Settings, Values } from './schema';
import { publicNameUrl } from './logic';

type Translate = (key: string, parameters?: Record<string, string | number>) => string;
const esc = (s: unknown) => String(s ?? '').replace(/[&<>"']/g, c => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' })[c]!);
export const TERMS_URL = 'https://letsencrypt.org/repository/';

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
export function publicNamePanel(status: Status, settings: Settings, busy: boolean, message: string, t: Translate, language: string): string {
  const values = settings.values;
  const enabled = values.publicNameEnabled === true;
  const tone = status.publicName?.state === 'error' ? 'error' : status.publicName?.state === 'ready' ? 'state-active' : 'muted';
  const files = publicNameFileErrors(status, t);
  return `${enabled ? `<p class="muted"><a href="${TERMS_URL}" target="_blank" rel="noopener">${esc(TERMS_URL)}</a></p>` : ''}<p id="public-name-status" class="${tone}" role="status" aria-live="polite">${esc(publicNameLine(status, t, language))}</p>${files.map(line => `<p class="error">${esc(line)}</p>`).join('')}<div class="public-name-actions">${enabled ? `<button id="public-name-renew" type="button" ${busy ? 'disabled' : ''}>${t('publicName.renew')}</button>` : ''}${publicNameConfigured(values, status.publicName) ? `<button id="public-name-remove" type="button" class="quiet" ${busy ? 'disabled' : ''}>${t('publicName.remove')}</button>` : ''}</div><p id="public-name-result" class="error" role="alert">${esc(message ? t(message) : '')}</p>`;
}
