// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-27

import { stepInfo, sectionView, type ConnectionPath, type StepText, type StepAction, type StepId, type StepState } from './model';
export interface StageFlags { countdown?: boolean; busy?: boolean; displayChanged?: boolean; carIncomplete?: boolean; setupCompleted?: boolean; overlay?: boolean }
export type StageAction = { type: 'Connect' | 'Stop' | 'NotNow' } | { type: 'Step'; action: StepAction; step: StepId; label: string | null };
export type Supporting = { type: 'LiveTimer'; sinceMillis: number | null } | { type: 'Elapsed'; sinceMillis: number } | { type: 'Evidence'; text: StepText; copy: boolean } | { type: 'Explanation'; res: string };
export function focusForPage(path: ConnectionPath) {
  return path.focus && ['FAILED', 'ACTION'].includes(path.focus.state) ? path.focus : path.sections.flatMap(s => s.steps).find(s => s.action === 'START_NOW') ?? path.focus;
}
export function presentStage(path: ConnectionPath, flags: StageFlags = {}) {
  const focus = focusForPage(path), live = path.live;
  const section = (id: string) => { const s = path.sections.find(s => s.section === id); return s ? sectionView(s.section, s.steps) : null; };
  const iphone = section('IPHONE'), relay = section('RELAY'), car = section('CAR'), picture = section('PICTURE');
  const link = (s: typeof car) => ({ done: s?.done ?? 0, total: s?.total ?? 0, state: s?.state ?? 'BLOCKED' as StepState });
  const focusSection = focus ? stepInfo[focus.id].section : null;
  const nodes = { iphone: iphone?.state ?? 'BLOCKED', relay: relay?.state ?? 'BLOCKED', car: picture?.state === 'OK' ? 'OK' : car?.state ?? 'BLOCKED', iphoneLink: link(iphone), carLink: link(car), focus: live ? null : focusSection === 'IPHONE' ? 'IPHONE' : focusSection === 'RELAY' ? 'RELAY' : focusSection ? 'CAR' : null, live: live != null };
  const headline: StepText = live ? { res: 'design_live', args: [] } : focus ? focus.detail ?? { res: focus.title, args: [] } : { res: !path.running && path.canStart ? 'design_ready' : 'design_idle', args: [] };
  const supporting: Supporting | null = live ? { type: 'LiveTimer', sinceMillis: live.sinceMillis } : focus?.state === 'FAILED' && focus.explanation ? { type: 'Explanation', res: focus.explanation } : focus?.evidence ? { type: 'Evidence', text: focus.evidence, copy: focus.id === 'CAR_PAGE' } : focus && ['WORKING', 'WAITING'].includes(focus.state) && focus.sinceMillis != null && focus.carWaitUntilMillis == null ? { type: 'Elapsed', sinceMillis: focus.sinceMillis } : null;
  const fix: StageAction | null = focus?.action && focus.action !== 'COPY_PAGE_ADDRESS' ? { type: 'Step', action: focus.action, step: focus.id, label: focus.actionLabel } : null;
  const countdown = !path.running && !!flags.countdown && path.canStart;
  const connect: StageAction = { type: 'Connect' }, stop: StageAction = { type: 'Stop' };
  const [primary, secondary]: [StageAction | null, StageAction | null] = countdown ? [connect, { type: 'NotNow' }] : !path.running && focus && ['ACTION', 'FAILED'].includes(focus.state) && fix ? [fix, path.canStart ? connect : null] : !path.running && path.canStart ? [connect, null] : !path.running ? [null, null] : live ? [null, stop] : [fix, stop];
  const notice = path.running && flags.displayChanged ? 'DISPLAY_CHANGED' : flags.carIncomplete ? 'CHOOSE_CAR' : flags.setupCompleted === false && !path.running ? 'SET_UP' : null;
  const failure = focus?.state === 'FAILED' && focus.code != null ? focus : null;
  return { nodes, headline, supporting, failure: failure?.code ?? null, primary, secondary, notice, segments: path.sections.map(s => ({ section: s.section, ...link(sectionView(s.section, s.steps)) })), failureSinceMillis: failure?.sinceMillis ?? null, primaryEnabled: !flags.busy, countdown, headlineDeadlineMillis: live ? null : focus?.carWaitUntilMillis ?? null };
}
export type StageView = ReturnType<typeof presentStage>;
export function stageTapSettled(changedAtMillis: number, nowMillis: number): boolean { return nowMillis - changedAtMillis >= 500; }
