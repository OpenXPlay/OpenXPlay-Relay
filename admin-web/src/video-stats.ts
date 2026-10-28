// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-21

import type { CarlinkStatus, Status, VideoStatus } from './api';
import type { Settings } from './schema';
import { codecName } from './diagnostics';
import { esc } from './html';
type Translate = (key: string, parameters?: Record<string, string | number>) => string;
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
export interface VideoCounters { frames: number; bytes: number; dropped: number; placeholders: number; stalls: number; relayDropped: number }
export type VideoMeasurement = { seconds: number } | { fps: number; bitsPerSecond: number; droppedPercent: number; placeholders: number; frames: number; sendStalls: number };
const progress = (window: number): VideoMeasurement => ({ seconds: window / 1000 });
const counter = (frames: number, bytes = 0, dropped = 0, placeholders = 0, stalls = 0, relayDropped = 0): VideoCounters => ({ frames, bytes, dropped, placeholders, stalls, relayDropped });
const number = (value: unknown): number | null => typeof value === 'number' && Number.isFinite(value) && value >= 0 ? value : null;

/** A disposable visible-page series, matching Android's one-second samples and 16-sample bound. */
export class VideoStatsSampler {
  private samples: { at: number; counters: VideoCounters }[] = [];
  private session = '';
  private window = 0;
  get retainedSamples(): number { return this.samples.length; }
  clear(): void { this.samples = []; this.session = ''; this.window = 0; }
  sample(session: string, window: number, counters: VideoCounters, now: number): void {
    if (!window) { this.clear(); return; }
    const previous = this.samples.at(-1);
    if (session !== this.session || window !== this.window || previous &&
      (now < previous.at || (Object.keys(counters) as (keyof VideoCounters)[]).some(key => counters[key] < previous.counters[key]))) this.samples = [];
    this.session = session; this.window = window;
    if (this.samples.length && now - this.samples.at(-1)!.at < 1000) return;
    if (this.samples.length === 16) this.samples.shift();
    this.samples.push({ at: now, counters: { ...counters } });
  }
  report(window: number): VideoMeasurement {
    if (window !== this.window) { this.samples = []; this.window = window; }
    const last = this.samples.at(-1);
    if (!last) return progress(window);
    const first = [...this.samples].reverse().find(sample => last.at - sample.at >= window);
    if (!first) return { seconds: Math.max(1, Math.ceil((window - (last.at - this.samples[0].at)) / 1000)) };
    const seconds = (last.at - first.at) / 1000;
    const frames = last.counters.frames - first.counters.frames;
    const dropped = last.counters.dropped - first.counters.dropped + last.counters.relayDropped - first.counters.relayDropped;
    return { fps: frames / seconds, bitsPerSecond: (last.counters.bytes - first.counters.bytes) * 8 / seconds,
      droppedPercent: frames + dropped === 0 ? 0 : dropped * 100 / (frames + dropped),
      frames, placeholders: last.counters.placeholders - first.counters.placeholders, sendStalls: last.counters.stalls - first.counters.stalls };
  }
}
export interface VideoStatistics {
  window: number; iphone: VideoMeasurement; format: VideoStatus; carlink: VideoMeasurement | null;
  clink: CarlinkStatus | null; placeholderActive: boolean;
  browsers: { measurement: VideoMeasurement; decodeMs: number | null; latencyMs: number | null }[];
}
export class VideoStatsSnapshot {
  private iphone = new VideoStatsSampler();
  private carlink = new VideoStatsSampler();
  private browsers = new Map<string, { sampler: VideoStatsSampler; relayDropped: number }>();
  private lastClink: VideoCounters | null = null;
  private placeholderActive = false;
  reset(): void { this.iphone.clear(); this.carlink.clear(); this.browsers.clear(); this.lastClink = null; this.placeholderActive = false; }
  read(status: Status, window: number, now: number): VideoStatistics | null {
    const video = status.video;
    const session = video?.session;
    if (![5000, 15000].includes(window) || !video?.active || !session) { this.reset(); return null; }
    const frames = number(video.frames), bytes = number(video.bytes);
    if (frames !== null && bytes !== null && video.bytesMeasured) this.iphone.sample(session, window, counter(frames, bytes), now);
    else this.iphone.clear();
    const carlinkOn = ['carlink', 'both'].includes(status.output);
    const clink = carlinkOn ? status.carlink : null;
    const stats = clink?.stats;
    if (stats && number(stats.liveFrames) !== null && number(stats.liveBytes) !== null) {
      const current = counter(Number(stats.liveFrames), Number(stats.liveBytes), number(stats.droppedFrames) ?? 0, number(stats.placeholderFrames) ?? 0, number(stats.sendStalls) ?? 0);
      const previous = this.lastClink;
      if (!previous || current.frames < previous.frames || current.placeholders < previous.placeholders) this.placeholderActive = current.placeholders > 0 && current.frames === 0;
      else if (current.frames > previous.frames) this.placeholderActive = false;
      else if (current.placeholders > previous.placeholders) this.placeholderActive = true;
      this.lastClink = current;
      this.carlink.sample(session, window, current, now);
    } else { this.carlink.clear(); this.lastClink = null; this.placeholderActive = false; }
    const reports = (status.receivers ?? []).filter(report => report.idleSeconds < 15).map(report => ({ report, stats: report.stats.at(-1) }))
      .filter(({ stats }) => stats && String(stats.session) === session && (stats.screen ?? 0) === 0 && number(stats.rendered) !== null && number(stats.dropped) !== null)
      .sort((a, b) => a.report.client < b.report.client ? -1 : a.report.client > b.report.client ? 1 : 0).slice(0, 4);
    const clients = new Set(reports.map(({ report }) => report.client));
    for (const key of this.browsers.keys()) if (!clients.has(key)) this.browsers.delete(key);
    const browsers = reports.map(({ report, stats: sample }) => {
      const stats = sample!;
      const browser = this.browsers.get(report.client) ?? { sampler: new VideoStatsSampler(), relayDropped: 0 };
      this.browsers.set(report.client, browser);
      browser.relayDropped = number(stats.relayDroppedFrames) ?? browser.relayDropped;
      browser.sampler.sample(session, window, counter(Number(stats.rendered), 0, Number(stats.dropped), 0, 0, browser.relayDropped), now);
      return { measurement: browser.sampler.report(window), decodeMs: number(stats.decodeMs), latencyMs: number(stats.renderedLatencyMs) };
    });
    return { window, iphone: this.iphone.report(window), format: { ...video }, carlink: carlinkOn ? this.carlink.report(window) : null,
      clink: clink ? structuredClone(clink) : null, browsers, placeholderActive: this.placeholderActive };
  }
}
export function videoStatsView(value: VideoStatistics | null, t: Translate): string {
  if (!value) return '';
  const decimal = (value: number) => value.toFixed(1);
  const size = (width: unknown, height: unknown) => number(width) && number(height) ? `${width} × ${height}` : '—';
  const ms = (value: number | null) => value === null ? '—' : `${Math.round(value)} ms`;
  const measuring = (value: { seconds: number }) => t('videoStats.measuring', { seconds: value.seconds });
  const section = (title: string, lines: string[]) => `<section><h3>${esc(title)}</h3>${lines.map(line => `<p>${esc(line)}</p>`).join('')}</section>`;
  const input = 'seconds' in value.iphone ? measuring(value.iphone) : t('videoStats.input', {
    codec: codecName(value.format.codec) ?? '—', size: size(value.format.width, value.format.height), fps: decimal(value.iphone.fps), mbps: decimal(value.iphone.bitsPerSecond / 1e6) });
  let sections = section(t('videoStats.iphone'), [input]);
  if (value.carlink) {
    const report = value.carlink, stats = value.clink?.stats;
    const line = 'seconds' in report ? measuring(report) : value.placeholderActive || report.placeholders > 0 && report.frames === 0 ? t('videoStats.placeholder') :
      t('videoStats.passthrough', { codec: codecName(String(stats?.codec ?? '')) || '—', size: size(stats?.width, stats?.height), fps: decimal(report.fps), dropped: decimal(report.droppedPercent) });
    const lines = [line];
    if (value.clink?.viewSize && number(stats?.refreshHz)) lines.push(t('videoStats.screen', { size: size(value.clink.viewSize.width, value.clink.viewSize.height), hz: stats!.refreshHz! }));
    if (!('seconds' in report) && report.sendStalls > 0) lines.push(t('videoStats.sendStalls', { count: report.sendStalls }));
    sections += section(t('videoStats.carlink'), lines);
  }
  value.browsers.forEach((browser, index) => {
    const report = browser.measurement;
    sections += section(t('videoStats.browser', { n: index + 1 }), ['seconds' in report ? measuring(report) : t('videoStats.browserValues', {
      fps: decimal(report.fps), dropped: decimal(report.droppedPercent), decode: ms(browser.decodeMs), latency: ms(browser.latencyMs) })]);
  });
  const caption = 'seconds' in value.iphone ? measuring(value.iphone) : t('videoStats.average', { seconds: value.window / 1000 });
  return `<article class="card" id="video-statistics"><h2>${t('field.videoStats')}</h2><p class="muted">${esc(caption)}</p>${sections}</article>`;
}
