// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-27

import { api, ApiError, NetworkError } from './api';
import { esc } from './html';
import type { Translate } from './stage';
export type UpdateChannel = 'stable' | 'beta';
export interface UpdateCandidate {
  latest: string | null; outcome: 'available' | 'up-to-date' | 'no-release';
  url: string | null; notes: string; publishedAt: string | null;
}
export interface UpdateCheck extends UpdateCandidate {
  current: string; channel: UpdateChannel; checkedMillis: number;
  candidates: Record<UpdateChannel, UpdateCandidate>;
}
export interface UpdateFailure { kind: 'offline' | 'timeout' | 'rate-limited' | 'http' | 'invalid-response'; resetMillis?: number | null; status?: number }
export type UpdateResponse = UpdateCheck | { channel: UpdateChannel; failure: UpdateFailure };
export class UpdateState {
  channel: UpdateChannel = 'stable'; checking = false;
  result: UpdateCheck | null = null; failure: UpdateFailure | null = null;
  private cached: UpdateCheck | null = null;
  private listeners = new Set<() => void>();
  constructor(private fetch: (channel: UpdateChannel) => Promise<UpdateResponse> = api.updateCheck) {
    try { if (localStorage.getItem('openxplay.updateChannel') === 'beta') this.channel = 'beta'; } catch {}
  }
  subscribe(listener: () => void): () => void { this.listeners.add(listener); return () => this.listeners.delete(listener); }
  private changed() { for (const listener of this.listeners) listener(); }
  setChannel(channel: UpdateChannel) {
    this.channel = channel;
    try { localStorage.setItem('openxplay.updateChannel', channel); } catch {}
    if (this.cached) {
      this.result = { ...this.cached, ...this.cached.candidates[channel], channel };
      this.failure = null;
    }
    this.changed();
  }
  async check() {
    if (this.checking) return;
    this.checking = true; this.failure = null; this.changed();
    try {
      const response = await this.fetch(this.channel);
      if ('failure' in response) { this.result = null; this.failure = response.failure; }
      else { this.cached = response; this.result = { ...response, ...response.candidates[this.channel], channel: this.channel }; }
    } catch (error) {
      this.result = null;
      this.failure = { kind: error instanceof NetworkError || error instanceof ApiError && error.key === 'offline' ? 'offline' : 'invalid-response' };
    } finally { this.checking = false; this.changed(); }
  }
}
export const updates = new UpdateState();
export function updateMenuVersion(version: string, t: Translate): string {
  return updates.result?.outcome === 'available' ? t('files.menuVersion', { current: version, version: updates.result.latest! }) : version;
}
export function updateFailureText(failure: UpdateFailure, t: Translate, locale: string): string {
  if (failure.kind === 'rate-limited') {
    const reset = failure.resetMillis == null ? null : new Date(failure.resetMillis);
    return reset && Number.isFinite(reset.getTime()) ? t('files.rateLimitedUntil', { time: reset.toLocaleString(locale) }) : t('files.rateLimited');
  }
  if (failure.kind === 'http') return t('files.httpError', { status: failure.status ?? 0 });
  return t({ offline: 'files.updateOffline', timeout: 'files.updateTimeout', 'invalid-response': 'files.invalidResponse' }[failure.kind]);
}
export interface NoteBlock { kind: 'heading' | 'bullet' | 'paragraph'; text: string }
export function localizedNotes(body: string, chinese: boolean): { summary: string; blocks: NoteBlock[] } {
  const lines = body.split(/\r?\n/); const provenance = lines.indexOf('---'); if (provenance >= 0) lines.splice(provenance);
  const summary = lines[0] ?? '', separator = summary.lastIndexOf(' · ');
  const han = (text: string) => /\p{Script=Han}/u.test(text);
  const heading = (text: string) => text.length > 4 && text.startsWith('**') && text.endsWith('**');
  const bilingual = separator >= 0 && han(summary.slice(separator + 3));
  const localizedSummary = bilingual ? chinese ? summary.slice(separator + 3) : summary.slice(0, separator) : summary;
  const split = lines.findIndex(line => heading(line.trim()) && han(line));
  const notes = split < 0 ? lines : chinese ? lines.slice(split) : lines.slice(1, split);
  const clean = (text: string) => text.replaceAll('**', '').replaceAll('`', '');
  const blocks: NoteBlock[] = [], paragraph: string[] = [];
  const flush = () => { if (paragraph.length) { blocks.push({ kind: 'paragraph', text: clean(paragraph.join(' ')) }); paragraph.length = 0; } };
  for (const raw of notes) {
    const line = raw.trim();
    if (!line) flush();
    else if (heading(line)) { flush(); blocks.push({ kind: 'heading', text: clean(line) }); }
    else if (line.startsWith('- ')) { flush(); blocks.push({ kind: 'bullet', text: clean(line.slice(2)) }); }
    else paragraph.push(line);
  }
  flush(); return { summary: clean(localizedSummary), blocks };
}
export function releaseMarkup(result: UpdateCheck, t: Translate, locale: string): string {
  if (result.outcome !== 'available') return `<p>${esc(t(result.outcome === 'no-release' ? 'files.noRelease' : 'files.upToDate', { version: result.latest ?? '' }))}</p>`;
  const notes = localizedNotes(result.notes, locale.startsWith('zh'));
  const url = safeReleaseUrl(result.url);
  const published = result.publishedAt ? new Date(result.publishedAt) : null;
  const date = published && Number.isFinite(published.getTime()) ? `${published.getFullYear()}-${String(published.getMonth() + 1).padStart(2, '0')}-${String(published.getDate()).padStart(2, '0')}` : '';
  return `<p>${esc(t('files.newVersion', { version: result.latest ?? '' }))}</p><time datetime="${esc(result.publishedAt ?? '')}">${esc(date)}</time><p>${esc(notes.summary)}</p>${url ? `<a href="${esc(url)}" target="_blank" rel="noopener noreferrer">${esc(t('files.release'))}</a>` : ''}<details data-key="release-notes"><summary>${esc(t('files.notes'))}</summary>${notes.blocks.map(block => block.kind === 'heading' ? `<h3>${esc(block.text)}</h3>` : block.kind === 'bullet' ? `<p>• ${esc(block.text)}</p>` : `<p>${esc(block.text)}</p>`).join('')}</details>`;
}
function safeReleaseUrl(value: string | null): string {
  if (!value) return '';
  try {
    const url = new URL(value);
    return url.protocol === 'https:' && url.hostname === 'github.com' && !url.username && !url.password && !url.port && !url.hash && !url.search && url.pathname.startsWith('/OpenXPlay/OpenXPlay-Relay/') && !/[\\%]/.test(value) ? url.href : '';
  } catch { return ''; }
}
