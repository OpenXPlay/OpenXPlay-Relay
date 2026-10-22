// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-21

import type { CarlinkStatus, Status } from './api';
import type { Settings } from './schema';
import { liveLine, windowLine } from './diagnostics';
type Translate = (key: string, parameters?: Record<string, string | number>) => string;
const esc = (s: unknown) => String(s ?? '').replace(/[&<>"']/g, c => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' })[c]!);
export function videoStatsInterval(settings: Settings | null, platform?: string): number {
  if (platform !== 'linux' || !settings?.schema.some(s => s.fields.some(f => f.key === 'videoStats'))) return 0;
  const value = settings.values.videoStats;
  return value === '5' ? 5000 : value === '15' ? 15000 : 0;
}
export function carlinkStatsView(c: CarlinkStatus | null, t: Translate): string {
  if (!c) return '';
  const view = c.viewSize ? `<p>${t('carlink.viewSize')}: ${esc(c.viewSize.width)} × ${esc(c.viewSize.height)}</p>` : '';
  const stats = c.stats;
  if (!stats) return view;
  const fields = ['sent', 'dropped', 'liveFrames', 'placeholderFrames', 'droppedFrames', 'liveBytes', 'sendStalls', 'codec', 'width', 'height'] as const;
  return view + `<dl class="facts">${fields.filter(key => stats[key] != null).map(key => `<dt>${t(`stats.${key}`)}</dt><dd>${esc(stats[key])}</dd>`).join('')}</dl>`;
}
export class VideoStatsSnapshot {
  private at = -Infinity;
  private interval = 0;
  private snapshot: Status | null = null;
  reset(): void { this.snapshot = null; this.interval = 0; }
  read(status: Status, interval: number, now: number): Status | null {
    const active = ['active', 'Active', 'Streaming', 'WirelessActive'].includes(status.state) || ['playing', 'active'].includes(status.carlink?.state ?? '');
    if (!interval || !active) { this.reset(); return null; }
    const previous = this.snapshot?.carlink?.stats;
    const current = status.carlink?.stats;
    const newSession = (previous && !current)
      || (typeof previous?.liveFrames === 'number' && typeof current?.liveFrames === 'number' && current.liveFrames < previous.liveFrames)
      || JSON.stringify(status.receivers?.map(r => r.client)) !== JSON.stringify(this.snapshot?.receivers?.map(r => r.client));
    if (!this.snapshot || newSession || interval !== this.interval || now - this.at >= interval) {
      this.snapshot = structuredClone(status); this.at = now; this.interval = interval;
    }
    return this.snapshot;
  }
}
export function videoStatsView(status: Status | null, t: Translate): string {
  if (!status) return '';
  return `<article class="card" id="video-statistics"><h2>${t('field.videoStats')}</h2>${carlinkStatsView(status.carlink, t)}${(status.receivers ?? []).map((r, i) => `<section><h3>${t('diag.screen', { n: i + 1 })}</h3><p>${esc(liveLine(r, t))}</p><p>${esc(windowLine(r, t))}</p></section>`).join('')}</article>`;
}
