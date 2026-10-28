// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-22

import { patchMarkup, confirmSheet } from './dom';
import { request } from './api';
export interface TraceSnapshot {
  enabled: boolean; running: boolean;
  status: { dropped: number; failure: string | null; capture: { state: string; bytes: number } } | null;
  files: { name: string; kind: 'trace' | 'media' | 'media-partial'; size: number }[];
}
type Translate = (key: string, parameters?: Record<string, string | number>) => string;
import { esc } from './html';
export const traceFilters = ['', 'Rtsp', 'Frame', 'Media', 'Link', 'Error'];
export function traceVisible(platform: string | undefined, developer: boolean, view: string): boolean { return platform === 'linux' && developer && view === 'settings'; }
function mediaSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${Number((bytes / 1024).toFixed(1))} KB`;
  return `${Number((bytes / (1024 * 1024)).toFixed(1))} MB`;
}
export function tracePanel(snapshot: TraceSnapshot | null, lines: string[], media: string | undefined, t: Translate, summary?: Record<string, string>): string {
  const capturing = snapshot?.status?.capture.state === 'capturing';
  const baseline = summary && Object.keys(summary).length ? `<pre class="trace-summary">${esc(JSON.stringify(summary, null, 2))}</pre>` : '';
  return `${baseline}<p>${esc(t('trace.counters', { events: Math.min(lines.length, 5000), dropped: snapshot?.status?.dropped ?? 0, bytes: snapshot?.status?.capture.bytes ?? 0 }))}</p>${snapshot?.status?.failure ? `<p class="error">${esc(t('trace.failure', { failure: snapshot.status.failure }))}</p>` : ''}${media ? `<pre>${esc(media)}</pre>` : ''}<div class="dialog-actions"><a class="button" ${snapshot?.files.some(f => f.kind === 'trace') ? 'href="/api/v0/carlink-trace/download" download' : 'aria-disabled="true"'}>${t('trace.download')}</a><button data-trace="clear">${t('trace.clear')}</button><button data-trace="capture" ${snapshot?.enabled || capturing ? '' : 'disabled'}>${t(capturing ? 'trace.stop' : 'trace.start')}</button></div><ul class="trace-files">${snapshot?.files.filter(f => f.kind === 'media').map(f => `<li><a class="button" href="/api/v0/carlink-trace/files/${encodeURIComponent(f.name)}" download>${esc(f.name)}</a><span class="muted">${mediaSize(f.size)}</span></li>`).join('') ?? ''}</ul><pre class="trace-lines">${lines.slice(0, 5000).map(esc).join('\n')}</pre>`;
}

/** At most one poll in flight; dispose invalidates it and cancels the next tick. */
export class TracePoller {
  private timer?: ReturnType<typeof setTimeout>;
  private stopped = false;
  private busy = false;
  constructor(private read: () => Promise<void>) {}
  start() { void this.refresh(); }
  async refresh() {
    if (this.stopped || this.busy) return;
    clearTimeout(this.timer); this.busy = true;
    try { if (typeof document === 'undefined' || !document.hidden) await this.read(); }
    finally { this.busy = false; if (!this.stopped) this.timer = setTimeout(() => void this.refresh(), 2000); }
  }
  dispose() { this.stopped = true; clearTimeout(this.timer); }
}
export function mountTrace(card: HTMLElement, t: Translate, errorKey: (error: unknown) => string, patch = patchMarkup, summary: () => Record<string, string> | undefined = () => undefined): () => void {
  let disposed = false, filter = '', snapshot: TraceSnapshot | null = null, busy = false;
  const select = card.querySelector<HTMLSelectElement>('select[data-trace-filter]')!;
  const content = card.querySelector<HTMLElement>('[data-trace-content]')!;
  const error = card.querySelector<HTMLElement>('[data-trace-error]')!;
  let revision = 0;
  const poller = new TracePoller(async () => {
    const current = revision;
    try {
      const [next, tail, lastMedia] = await Promise.all([
        request<TraceSnapshot>('/carlink-trace'),
        request<{ lines: string[] }>(`/carlink-trace/tail?limit=5000${filter ? `&type=${filter}` : ''}`),
        request<{ lines: string[] }>('/carlink-trace/tail?type=Media&limit=1'),
      ]);
      if (disposed || current !== revision) return;
      const scrollTop = content.querySelector<HTMLElement>('.trace-lines')?.scrollTop ?? 0;
      const focused = content.ownerDocument.activeElement;
      const action = content.querySelectorAll<HTMLButtonElement>('button[data-trace]');
      const focusKey = Array.from(action).find(button => button === focused)?.dataset.trace;
      snapshot = next; patch(content, tracePanel(next, tail.lines, lastMedia.lines[0], t, summary()));
      content.querySelectorAll<HTMLButtonElement>('button').forEach(b => { b.disabled ||= busy; });
      if (focusKey) Array.from(content.querySelectorAll<HTMLButtonElement>('button[data-trace]')).find(button => button.dataset.trace === focusKey)?.focus({ preventScroll: true });
      const list = content.querySelector<HTMLElement>('.trace-lines');
      if (list) list.scrollTop = scrollTop;
    } catch (e) { if (!disposed) error.textContent = t(errorKey(e)); }
  });
  select.addEventListener('change', () => { filter = select.value; revision++; void poller.refresh(); });
  content.addEventListener('click', async event => {
    const action = (event.target as HTMLElement).closest<HTMLButtonElement>('button[data-trace]')?.dataset.trace;
    if (!action || busy || (action === 'clear' && !await confirmSheet(t('trace.clearConfirm'), t('trace.clear'), t('ui.cancel')))) return;
    busy = true; error.textContent = '';
    content.querySelectorAll<HTMLButtonElement>('button').forEach(b => { b.disabled = true; });
    try {
      await request(`/carlink-trace/${action}`, 'POST', action === 'capture' ? { action: snapshot?.status?.capture.state === 'capturing' ? 'stop' : 'start' } : {}, false);
    } catch (e) { if (!disposed) error.textContent = t(errorKey(e)); }
    finally { busy = false; if (!disposed) void poller.refresh(); }
  });
  poller.start();
  return () => { disposed = true; poller.dispose(); };
}
