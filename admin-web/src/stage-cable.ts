// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-30

import { esc } from './html';
import type { StepState } from './path/model';
import type { Translate } from './stage';
export interface LinkFill { done: number; total: number; state: StepState }
const curve = 'M0 12 C30 22 70 22 100 12';
const reverseCurve = 'M100 12 C70 22 30 22 0 12';
export function cableMarkup(fill: LinkFill, left: boolean, live: boolean, label: string, t: Translate): string {
  const fraction = live ? 1 : fill.total ? fill.done / fill.total : 0;
  const broken = fill.state === 'FAILED' && !live;
  const end = Math.max(.08, Math.min(.92, fraction));
  const track = broken ? `${end - .02} .04 ${1 - end - .02}` : fill.state === 'BLOCKED' && !live ? '.05 .04' : '1 0';
  const progress = broken ? `${Math.min(fraction, end - .02)} ${1 - Math.min(fraction, end - .02)}` : `${fraction} 1`;
  return `<span class="cable-count" ${live ? 'data-live' : ''}>${fill.total ? esc(t('stage_progress_count', { arg1: fill.done, arg2: fill.total })) : ''}</span><svg viewBox="0 0 100 32" preserveAspectRatio="none" aria-hidden="true"><g ${left ? 'transform="translate(100 0) scale(-1 1)"' : ''}><path class="cable-track" d="${curve}" pathLength="1" stroke-dasharray="${track}"/><path class="cable-fill" d="${curve}" pathLength="1" stroke-dasharray="${progress}"/><path class="cable-plug near" d="M0 8 V16"/><path class="cable-plug far" data-complete="${live || fill.total > 0 && fill.done === fill.total}" d="M100 8 V16"/></g></svg><span class="sr-only">${esc(t('stage_link_progress', { arg1: label, arg2: fill.done, arg3: fill.total }))}</span>`;
}
export function cableMotion(previous: LinkFill | null, next: LinkFill, wasLive: boolean, live: boolean) {
  return { sparks: previous ? Math.max(0, next.done - previous.done) : 0,
    click: !!previous && next.total > 0 && next.done === next.total && (previous.done !== next.done || previous.total !== next.total), pulse: live && !wasLive };
}
/** SVG motion has a finite lifetime; unchanged polls never schedule another pass. */
export function playCableMotion(element: HTMLElement, left: boolean, motion: ReturnType<typeof cableMotion>, reduced: boolean) {
  if (reduced) return;
  const svg = element.querySelector('svg')!;
  const pass = (path: string, delay: number, duration: number, kind: string) => {
    const dot = document.createElementNS('http://www.w3.org/2000/svg', 'circle');
    dot.setAttribute('r', kind === 'spark' ? '3' : '5'); dot.setAttribute('class', `cable-${kind}`); dot.setAttribute('opacity', '0');
    const travel = document.createElementNS('http://www.w3.org/2000/svg', 'animateMotion');
    travel.setAttribute('path', path); travel.setAttribute('dur', `${duration}s`); travel.setAttribute('begin', 'indefinite'); travel.setAttribute('repeatCount', '1');
    travel.addEventListener('beginEvent', () => dot.setAttribute('opacity', '.65'));
    travel.addEventListener('endEvent', () => dot.remove(), { once: true }); dot.append(travel); svg.append(dot); travel.beginElementAt?.(delay);
  };
  for (let i = 0; i < motion.sparks; i++) pass(left ? reverseCurve : curve, i * .18, .48, 'spark');
  if (motion.click) element.querySelector('.far')?.animate?.([{ strokeWidth: 6 }, { strokeWidth: 9 }, { strokeWidth: 6 }], { duration: 230 });
  if (motion.pulse) for (let i = 0; i < 2; i++) pass(curve, i * 1.6 + (left ? 0 : .65), .65, 'pulse');
}
