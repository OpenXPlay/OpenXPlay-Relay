// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-27

import { icon } from './icons';
import { esc } from './html';
import { patchMarkup, on } from './dom';
import { normalizeInput, type PartialInput, type PathSection, type StepText } from './path/model';
import { countdownSeconds } from './path/time';
import { presentPath, clockTime } from './path/present-path';
import { presentStage, stageTapSettled, type StageAction, type StageView } from './path/present-stage';
import { stepCommand } from './path/actions';
export type Translate = (key: string, args?: Record<string, string | number>) => string;
export function stepText(value: StepText | null | undefined, t: Translate): string {
  if (!value) return '';
  return value.raw ?? (value.res ? t(value.res, Object.fromEntries((value.args ?? []).map((v, i) => [`arg${i + 1}`, v]))) : '');
}
export function elapsed(since: number | null, now: number): number { return since == null ? 0 : Math.max(0, Math.floor((now - since) / 1000)); }
export function timer(seconds: number): string { return `${String(Math.floor(seconds / 60)).padStart(2, '0')}:${String(seconds % 60).padStart(2, '0')}`; }
const glyphs = { OK: '✓', WORKING: '•', WAITING: '○', ACTION: '!', FAILED: '×', BLOCKED: '○' };
export function mountStage(target: HTMLElement, options: { t: Translate; action: (action: StageAction) => void; copy: (text: string) => void; details: () => string; bindDetails: (root: HTMLElement) => void; notice: (notice: string) => void }) {
  const t = options.t;
  target.innerHTML = `<div class="stage-page"><div class="stage-main"><div class="stage-story"><div class="connection-stage" data-key="nodes">${['IPHONE', 'RELAY', 'CAR'].map((name, i) => `${i ? `<span class="stage-link ${i === 1 ? 'left' : ''}" data-link="${i}" aria-hidden="true"><i></i></span>` : ''}<button class="stage-tile ${name === 'RELAY' ? 'relay' : ''}" data-node="${name}"><span class="node-icon"></span><span class="stage-badge" aria-hidden="true"></span></button>`).join('')}</div><div class="stage-text"><h1 id="stage-headline" aria-live="polite"></h1><p id="stage-supporting" class="muted"></p><button id="stage-copy" class="quiet" hidden></button><p id="stage-failure" class="mono error"></p><button id="stage-notice" class="notice-chip" hidden></button></div></div><div class="stage-actions"><button id="stage-primary" class="primary pill" hidden></button><button id="stage-secondary" class="quiet" hidden></button><p id="action-result" class="error" role="alert"></p></div></div><button class="drawer-handle" id="drawer-open" aria-haspopup="dialog" aria-expanded="false" aria-label="${esc(t('stage_steps'))}"><span class="drawer-segments"></span>${icon('chevron-down')}</button></div>`;
  let input = normalizeInput(), path = presentPath(input), view = presentStage(path), received = performance.now();
  let actionChanged = 0, actionSignature = '', timerVisible = false;
  let drawer: HTMLDialogElement | null = null;
  const node = <T extends HTMLElement = HTMLElement>(id: string) => target.querySelector<T>(`#${id}`)!;
  const now = () => input.nowMillis + performance.now() - received;
  const label = (action: StageAction) => action.type === 'Step' ? t(action.label ?? 'design_open') : t(action.type === 'Connect' ? 'stage.connect' : action.type === 'Stop' ? 'design_stop' : 'stage_not_now');
  const permitted = (action: StageAction | null) => action && (action.type !== 'Step' || stepCommand(action.action, input.platform)) ? action : null;
  const updateTimers = () => {
    let headline = stepText(view.headline, t);
    if (view.headlineDeadlineMillis != null) headline = t(view.headline.res!, { arg1: countdownSeconds(view.headlineDeadlineMillis, now()) });
    if (node('stage-headline').textContent !== headline) { node('stage-headline').textContent = headline; node('stage-headline').animate?.([{ opacity: .3, transform: 'translateY(8px)' }, { opacity: 1, transform: 'translateY(0)' }], { duration: matchMedia('(prefers-reduced-motion: reduce)').matches ? 0 : 150 }); }
    const s = view.supporting;
    const supporting = s?.type === 'Evidence' ? stepText(s.text, t) : s?.type === 'Explanation' ? t(s.res) : s?.type === 'LiveTimer' ? timer(elapsed(s.sinceMillis, now())) : s?.type === 'Elapsed' ? t('design_seconds', { arg1: elapsed(s.sinceMillis, now()) }) : '';
    node('stage-supporting').textContent = s?.type === 'Evidence' && s.copy ? '' : supporting;
    node('stage-copy').hidden = !(s?.type === 'Evidence' && s.copy); node('stage-copy').textContent = `${supporting} ⧉`;
  };
  on(node('stage-copy'), 'click', () => { if (view.supporting?.type === 'Evidence') options.copy(stepText(view.supporting.text, t)); });
  on(node('stage-primary'), 'click', () => { const a = permitted(view.primary); if (a && stageTapSettled(actionChanged, performance.now())) options.action(a); });
  on(node('stage-secondary'), 'click', () => { const a = permitted(view.secondary); if (a && stageTapSettled(actionChanged, performance.now())) options.action(a); });
  on(node('stage-notice'), 'click', () => { if (view.notice) options.notice(view.notice); });
  function drawDetails() {
    if (!drawer) return;
    const sectionMarkup = path.sections.map(section => `<details class="path-section" data-key="${section.section}" id="path-${section.section}" ${section.state !== 'OK' && section.state !== 'BLOCKED' ? 'open' : ''}><summary><span class="glyph" data-state="${section.state}">${glyphs[section.state]}</span><strong>${esc(t(section.section === 'RELAY' && input.platform === 'linux' ? 'stage.relay' : `path_section_${section.section.toLowerCase()}`))}</strong><span class="mono muted">${section.done}/${section.total}</span>${icon('chevron-down')}</summary><div class="path-steps">${section.done && section.done !== section.total ? `<details class="done-steps" data-key="done"><summary>${esc(t('design_done_count', { arg1: section.done }))} · ${esc(section.steps.filter(s => s.state === 'OK').map(s => t(s.title)).join(' · '))}</summary>${section.steps.filter(s => s.state === 'OK').map(row).join('')}</details>` : ''}${section.steps.filter(s => !section.done || section.done === section.total || s.state !== 'OK').map(row).join('')}</div></details>`).join('');
    function row(s: typeof path.focus): string {
      if (!s) return '';
      const command = s.action && stepCommand(s.action, input.platform);
      return `<div class="step-row" data-step="${s.id}" data-state="${s.state}"><span class="glyph">${glyphs[s.state]}</span><button class="step-detail quiet" data-detail="${s.id}"><strong>${esc(t(s.title))}</strong><span class="muted">${esc(stepText(s.detail, t))}</span><span class="mono muted">${esc(stepText(s.evidence, t))}</span></button>${command ? `<button class="step-action" data-step-action="${s.id}">${esc(t(s.actionLabel!))}</button>` : ''}</div>`;
    }
    patchMarkup(drawer.querySelector('#path-sections')!, sectionMarkup);
    patchMarkup(drawer.querySelector('#drawer-details')!, options.details());
    drawer.querySelectorAll<HTMLButtonElement>('[data-step-action]').forEach(button => on(button, 'click', () => {
      const step = path.sections.flatMap(s => s.steps).find(s => s.id === button.dataset.stepAction);
      if (step?.action) { closeDrawer(); options.action({ type: 'Step', action: step.action, step: step.id, label: step.actionLabel }); }
    }));
    drawer.querySelectorAll<HTMLButtonElement>('[data-detail]').forEach(button => on(button, 'click', () => {
      const s = path.sections.flatMap(s => s.steps).find(s => s.id === button.dataset.detail)!;
      const dialog = document.createElement('dialog'); dialog.setAttribute('aria-label', t(s.title));
      dialog.innerHTML = `<h2>${esc(t(s.title))}</h2><p>${esc(stepText(s.detail, t))}</p><p class="mono">${esc(stepText(s.evidence, t))}</p>${s.explanation ? `<p>${esc(t(s.explanation))}</p>` : ''}<pre>${esc(s.trail.join('\n'))}</pre><button data-close>${esc(t('ui.close'))}</button>`;
      document.body.append(dialog); dialog.showModal(); on(dialog.querySelector('[data-close]'), 'click', () => { dialog.close(); dialog.remove(); button.focus(); });
      on(dialog, 'cancel', e => { e.preventDefault(); dialog.close(); dialog.remove(); button.focus(); });
    }));
    options.bindDetails(drawer);
  }
  function closeDrawer() { if (!drawer) return; drawer.close(); drawer.remove(); drawer = null; node('drawer-open').setAttribute('aria-expanded', 'false'); node('drawer-open').focus(); }
  function openDrawer(section?: PathSection) {
    if (!drawer) {
      drawer = document.createElement('dialog'); drawer.id = 'stage-drawer'; drawer.setAttribute('aria-label', t('stage_steps'));
      drawer.innerHTML = `<button class="drawer-handle" id="drawer-close" aria-label="${esc(t('ui.close'))}"><span class="drawer-segments"></span>${icon('chevron-down')}</button><div class="drawer-body"><div id="path-sections"></div><div id="drawer-details"></div><p class="mono muted" id="path-diagnostic"></p></div>`;
      document.body.append(drawer); drawer.showModal(); node('drawer-open').setAttribute('aria-expanded', 'true'); on(drawer, 'cancel', e => { e.preventDefault(); closeDrawer(); }); on(drawer.querySelector('#drawer-close'), 'click', closeDrawer);
      let dragY = 0; const handle = drawer.querySelector<HTMLElement>('#drawer-close')!;
      on(handle, 'pointerdown', e => { dragY = e.clientY; handle.setPointerCapture(e.pointerId); });
      on(handle, 'pointerup', e => { if (e.clientY - dragY > 40) closeDrawer(); }); drawDetails(); updateSegments();
    }
    if (section) { const card = drawer.querySelector<HTMLDetailsElement>(`#path-${section}`); if (card) { card.open = true; card.scrollIntoView({ block: 'start', behavior: 'instant' }); } }
  }
  function updateSegments() {
    const markup = view.segments.map(s => `<i data-key="${s.section}" data-state="${s.state}" style="flex:${s.total || 1};--fill:${s.total ? s.done / s.total : 0}"></i>`).join('');
    target.querySelectorAll('.drawer-segments').forEach(e => patchMarkup(e, markup));
    drawer?.querySelectorAll('.drawer-segments').forEach(e => patchMarkup(e, markup));
    if (drawer) drawer.querySelector('#path-diagnostic')!.textContent = `${path.diagnostic} | ${path.version}`;
  }
  on(node('drawer-open'), 'click', () => openDrawer());
  let drag = 0; node('drawer-open').addEventListener('pointerdown', e => { drag = e.clientY; (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId); });
  node('drawer-open').addEventListener('pointerup', e => { if (drag - e.clientY > 24) openDrawer(); });
  target.querySelectorAll<HTMLButtonElement>('[data-node]').forEach(tile => on(tile, 'click', () => openDrawer(tile.dataset.node === 'CAR' ? path.sections.some(s => s.section === 'CAR') ? 'CAR' : 'PICTURE' : tile.dataset.node as PathSection)));
  return {
    update(partial: PartialInput, flags: Parameters<typeof presentStage>[1] = {}, error = '') {
      input = normalizeInput(partial); received = performance.now(); path = presentPath(input); view = presentStage(path, flags);
      const signature = JSON.stringify([permitted(view.primary), permitted(view.secondary)]);
      if (signature !== actionSignature) { actionChanged = performance.now(); actionSignature = signature; }
      for (const name of ['IPHONE', 'RELAY', 'CAR'] as const) {
        const tile = target.querySelector<HTMLButtonElement>(`[data-node="${name}"]`)!;
        const state = view.nodes[name.toLowerCase() as 'iphone' | 'relay' | 'car'];
        tile.dataset.state = state; tile.classList.toggle('unfocused', !!view.nodes.focus && view.nodes.focus !== name);
        tile.setAttribute('aria-label', [t(name === 'IPHONE' ? 'ui.iphone' : name === 'RELAY' ? input.platform === 'linux' ? 'stage.relay' : 'stage.phone' : 'stage.car'), t(`stage.state.${state}`), path.nodes[name.toLowerCase() as 'iphone' | 'relay' | 'car']].filter(Boolean).join(', '));
        const iconName = name === 'CAR' ? 'car-front' : name === 'RELAY' && input.platform === 'linux' ? 'router' : 'smartphone';
        if (tile.dataset.icon !== iconName) { patchMarkup(tile.querySelector('.node-icon')!, icon(iconName)); tile.dataset.icon = iconName; }
        tile.querySelector<HTMLElement>('.stage-badge')!.hidden = state === 'OK'; tile.querySelector('.stage-badge')!.textContent = glyphs[state];
      }
      target.querySelectorAll<HTMLElement>('[data-link]').forEach((link, i) => { const fill = i === 0 ? view.nodes.iphoneLink : view.nodes.carLink; link.dataset.state = view.nodes.live ? 'OK' : fill.state; link.style.setProperty('--fill', String(view.nodes.live ? 1 : fill.total ? fill.done / fill.total : 0)); });
      target.querySelector('.connection-stage')!.classList.toggle('live', view.nodes.live);
      updateTimers(); node('stage-failure').textContent = view.failure ? `${view.failure}${view.failureSinceMillis != null ? ` · ${clockTime(view.failureSinceMillis)}` : ''}` : '';
      for (const [id, action] of [['stage-primary', permitted(view.primary)], ['stage-secondary', permitted(view.secondary)]] as const) { const button = node<HTMLButtonElement>(id); button.hidden = !action; if (action) button.textContent = label(action); button.dataset.action = action?.type === 'Step' ? action.action : action?.type ?? ''; if (id === 'stage-primary') button.style.width = action?.type === 'Connect' ? '100%' : '94%'; button.disabled = !!flags.busy; }
      node('stage-notice').hidden = !view.notice; node('stage-notice').textContent = view.notice ? t(view.notice === 'CHOOSE_CAR' ? 'stage_notice_car' : view.notice === 'DISPLAY_CHANGED' ? 'stage_notice_display' : view.notice === 'TRUSTED_HTTPS' ? 'car.zeekrHttps' : 'ui.settings') : '';
      node('action-result').textContent = error ? t(error) : '';
      updateSegments(); if (drawer) drawDetails();
      timerVisible = view.headlineDeadlineMillis != null || view.supporting?.type === 'LiveTimer' && view.supporting.sinceMillis != null || view.supporting?.type === 'Elapsed';

    },
    tick: updateTimers, get timerVisible() { return timerVisible; },
    destroy() { if (drawer) closeDrawer(); },
  };
}
