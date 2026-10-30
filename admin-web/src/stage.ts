// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-27

import './stage.css';
import { cableMarkup, cableMotion, playCableMotion, type LinkFill } from './stage-cable';
import { stepDetailsView } from './step-details';
import { arrivalCountdown, arrivalRemaining } from './startup';
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
  return value.raw ?? (value.res ? t(value.res, Object.fromEntries((value.args ?? []).map((v, i) => [`arg${i + 1}`, value.res === 'path_start_when' ? t(v) : v]))) : '');
}
export function elapsed(since: number | null, now: number): number { return since == null ? 0 : Math.max(0, Math.floor((now - since) / 1000)); }
export function timer(seconds: number): string { return `${String(Math.floor(seconds / 60)).padStart(2, '0')}:${String(seconds % 60).padStart(2, '0')}`; }
const glyphs = { OK: '✓', WORKING: '•', WAITING: '○', ACTION: '!', FAILED: '×', BLOCKED: '○' };
export function mountStage(target: HTMLElement, options: { t: Translate; action: (action: StageAction) => void; copy: (text: string) => Promise<boolean>; details: () => string; bindDetails: (root: HTMLElement) => void; notice: (notice: string) => void; recheck: () => Promise<void>; addresses: () => string[]; opened: () => void }) {
  const t = options.t;
  const reduced = matchMedia('(prefers-reduced-motion: reduce)');
  let previousLinks: LinkFill[] | null = null, wasLive = false;
  let nudge: Animation | undefined;
  const snapMotion = () => {
    if (!reduced.matches) return;
    nudge?.cancel(); target.getAnimations({ subtree: true }).forEach(animation => animation.cancel());
    target.querySelectorAll('animateMotion').forEach(motion => motion.parentElement?.remove());
  };
  reduced.addEventListener('change', snapMotion);
  target.innerHTML = `<div class="stage-page"><div class="stage-main"><div class="stage-story"><div class="connection-stage" data-key="nodes">${['IPHONE', 'RELAY', 'CAR'].map((name, i) => `${i ? `<span class="stage-link ${i === 1 ? 'left' : ''}" data-link="${i}" role="img"></span>` : ''}<button class="stage-tile ${name === 'RELAY' ? 'relay' : ''}" data-node="${name}">${name === 'RELAY' ? '<svg class="relay-progress" viewBox="0 0 76 76" aria-hidden="true"><circle cx="38" cy="38" r="37"/><circle class="relay-fill" cx="38" cy="38" r="37" pathLength="1"/></svg>' : ''}<span class="node-icon"></span><span class="stage-badge" aria-hidden="true"></span></button>`).join('')}</div><div class="stage-text"><h1 id="stage-headline" aria-live="polite"></h1><p id="stage-supporting" class="muted"></p><button id="stage-copy" class="quiet" hidden></button><p id="stage-failure" class="mono error"></p><button id="stage-notice" class="notice-chip" hidden></button></div></div><div class="stage-actions"><div class="arrival-action"><button id="stage-primary" class="primary pill" hidden></button><svg id="arrival-ring" aria-hidden="true" hidden><rect x="4" y="4" rx="24" pathLength="1" /></svg></div><p id="arrival-countdown" class="muted" role="timer" hidden></p><button id="stage-secondary" class="quiet" hidden></button><p id="action-result" class="error" role="alert"></p></div></div><button class="drawer-handle" id="drawer-open" aria-haspopup="dialog" aria-expanded="false" aria-label="${esc(t('stage_steps'))}"><span class="drawer-segments"></span><span class="drawer-hint"><span>${esc(t('stage_swipe_hint'))}</span><span class="drawer-count"></span>${icon('chevron-down')}</span></button></div>`;
  let input = normalizeInput(), path = presentPath(input), view = presentStage(path), received = performance.now();
  let actionChanged = 0, actionSignature = '', timerVisible = false;
  let drawer: HTMLDialogElement | null = null;
  let stepDialog: HTMLDialogElement | null = null, selectedStep = "";
  let countdownRefreshed = false;
  const node = <T extends HTMLElement = HTMLElement>(id: string) => target.querySelector<T>(`#${id}`)!;
  const now = () => input.nowMillis + performance.now() - received;
  const label = (action: StageAction) => action.type === 'Step' ? t(action.label ?? 'design_open') : t(action.type === 'Connect' ? 'stage.connect' : action.type === 'Stop' ? 'design_stop' : action.type === 'PauseAutomatic' ? 'stage_dont_auto' : 'stage_not_now');
  const permitted = (action: StageAction | null) => action && (action.type !== 'Step' || stepCommand(action.action, input.platform)) ? action : null;
  const updateTimers = () => {
    let headline = stepText(view.headline, t);
    if (view.headlineDeadlineMillis != null) headline = t(view.headline.res!, { arg1: countdownSeconds(view.headlineDeadlineMillis, now()) });
    if (node('stage-headline').textContent !== headline) { node('stage-headline').textContent = headline; node('stage-headline').animate?.([{ opacity: .3, transform: 'translateY(8px)' }, { opacity: 1, transform: 'translateY(0)' }], { duration: matchMedia('(prefers-reduced-motion: reduce)').matches ? 0 : 150 }); }
    const remaining = arrivalRemaining(input.startup, input.nowMillis, performance.now() - received);
    const counting = view.countdown && remaining > 0;
    node('arrival-ring').toggleAttribute('hidden', !counting);
    node('arrival-countdown').hidden = !counting;
    node('arrival-countdown').textContent = counting ? t('stage_auto_in', { arg1: Math.ceil(remaining / 1000) }) : '';
    node('stage-primary').setAttribute('aria-describedby', counting ? 'arrival-countdown' : 'stage-supporting');
    target.querySelector('rect')?.setAttribute('stroke-dasharray', `${remaining / 5000} 1`);
    target.querySelector('rect')?.setAttribute('rx', String(Math.max(0, node('stage-primary').clientHeight / 2 - 4)));
    if (view.countdown && !counting && !countdownRefreshed) { countdownRefreshed = true; void options.recheck(); }
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
      selectedStep = s.id;
      stepDialog = document.createElement('dialog'); stepDialog.setAttribute('aria-label', t(s.title));
      stepDialog.innerHTML = '<div data-step-content></div>'; document.body.append(stepDialog); drawStep(); stepDialog.showModal();
      on(stepDialog, 'cancel', e => { e.preventDefault(); closeStep(); button.focus(); });
    }));
    options.bindDetails(drawer);
  }
  function closeStep() { stepDialog?.close(); stepDialog?.remove(); stepDialog = null; selectedStep = ''; }
  function drawStep() {
    if (!stepDialog) return;
    const s = path.sections.flatMap(section => section.steps).find(step => step.id === selectedStep);
    if (!s) { closeStep(); return; }
    patchMarkup(stepDialog.querySelector('[data-step-content]')!, stepDetailsView(s, options.addresses(), input.platform, t));
    stepDialog.querySelectorAll<HTMLButtonElement>('[data-evidence]').forEach(button => on(button, 'click', async () => { if (await options.copy(button.dataset.evidence!)) button.textContent = t('design_copied'); }));
    on(stepDialog.querySelector('[data-close]'), 'click', closeStep);
    on(stepDialog.querySelector('[data-action]'), 'click', () => { closeStep(); closeDrawer(); if (s.action) options.action({ type: 'Step', action: s.action, step: s.id, label: s.actionLabel }); });
    on(stepDialog.querySelector('[data-recheck]'), 'click', async () => { await options.recheck(); if (stepDialog) stepDialog.querySelector('[data-recheck-result]')!.textContent = t('ui.saved'); });
  }
  function closeDrawer() { if (!drawer) return; closeStep(); drawer.close(); drawer.remove(); drawer = null; node('drawer-open').setAttribute('aria-expanded', 'false'); node('drawer-open').focus(); }
  function openDrawer(section?: PathSection) {
    if (!drawer) {
      nudge?.cancel();
      options.opened();
      drawer = document.createElement('dialog'); drawer.id = 'stage-drawer'; drawer.setAttribute('aria-label', t('stage_steps'));
      drawer.innerHTML = `<button class="drawer-handle" id="drawer-close" aria-label="${esc(t('ui.close'))}"><span class="drawer-segments"></span>${icon('chevron-down')}</button><div class="drawer-body"><div id="path-sections"></div><div id="drawer-details"></div><button class="mono muted quiet" id="path-diagnostic" aria-label="${esc(t('design_copy_diagnostic'))}"></button></div>`;
      document.body.append(drawer); drawer.showModal(); on(drawer.querySelector('#path-diagnostic'), 'click', async () => { if (await options.copy(`${path.diagnostic} | ${path.version}`) && drawer) drawer.querySelector('#path-diagnostic')!.textContent = t('design_copied'); }); node('drawer-open').setAttribute('aria-expanded', 'true'); on(drawer, 'cancel', e => { e.preventDefault(); closeDrawer(); }); on(drawer.querySelector('#drawer-close'), 'click', closeDrawer);
      let dragY = 0; const handle = drawer.querySelector<HTMLElement>('#drawer-close')!;
      on(handle, 'pointerdown', e => { dragY = e.clientY; handle.setPointerCapture(e.pointerId); });
      on(handle, 'pointerup', e => { if (e.clientY - dragY > 40) closeDrawer(); }); drawDetails(); updateSegments();
    }
    if (section) { const card = drawer.querySelector<HTMLDetailsElement>(`#path-${section}`); if (card) { card.open = true; card.scrollIntoView({ block: 'start', behavior: 'instant' }); } }
  }
  function updateSegments() {
    node('drawer-open').setAttribute('aria-label', t('stage_steps_progress', { arg1: view.done, arg2: view.total }));
    const count = target.querySelector('.drawer-count')!;
    count.textContent = t('stage_progress_count', { arg1: view.done, arg2: view.total });
    const markup = view.segments.map(s => `<i data-key="${s.section}" data-state="${s.state}" style="flex:${s.total || 1};--fill:${s.total ? s.done / s.total : 0}"></i>`).join('');
    target.querySelectorAll('.drawer-segments').forEach(e => patchMarkup(e, markup));
    drawer?.querySelectorAll('.drawer-segments').forEach(e => patchMarkup(e, markup));
    if (drawer) drawer.querySelector('#path-diagnostic')!.textContent = `${path.diagnostic} | ${path.version}`;
  }
  on(node('drawer-open'), 'click', () => openDrawer());
  let drag = 0; node('drawer-open').addEventListener('pointerdown', e => { drag = e.clientY; (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId); });
  node('drawer-open').addEventListener('pointerup', e => { if (drag - e.clientY > 24) openDrawer(); });
  target.querySelectorAll<HTMLButtonElement>('[data-node]').forEach(tile => on(tile, 'click', () => openDrawer(tile.dataset.node === 'CAR' ? path.sections.some(s => s.section === 'CAR') ? 'CAR' : 'PICTURE' : tile.dataset.node as PathSection)));
  if (!reduced.matches) nudge = target.querySelector('.drawer-hint')?.animate?.([{ transform: 'translateY(0)' }, { transform: 'translateY(-6px)', offset: .2 }, { transform: 'translateY(0)', offset: .5 }, { transform: 'translateY(-6px)', offset: .7 }, { transform: 'translateY(0)' }], { delay: 600, duration: 900, iterations: 1 });
  return {
    update(partial: PartialInput, flags: Parameters<typeof presentStage>[1] = {}, error = '') {
      input = normalizeInput(partial); received = performance.now(); path = presentPath(input); view = presentStage(path, { ...flags, countdown: arrivalCountdown(input.startup, path, flags.busy) });
      if (!view.countdown) countdownRefreshed = false;
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
      const links = [view.nodes.iphoneLink, view.nodes.carLink];
      target.querySelectorAll<HTMLElement>('[data-link]').forEach((link, i) => {
        const fill = links[i], label = t(i === 0 ? 'ui.iphone' : 'stage.car');
        link.dataset.state = view.nodes.live ? 'OK' : fill.state;
        link.setAttribute('aria-label', t('stage_link_progress', { arg1: label, arg2: fill.done, arg3: fill.total }));
        if (!previousLinks || JSON.stringify(previousLinks[i]) !== JSON.stringify(fill) || wasLive !== view.nodes.live) patchMarkup(link, cableMarkup(fill, i === 0, view.nodes.live, label, t));
        playCableMotion(link, i === 0, cableMotion(previousLinks?.[i] ?? null, fill, wasLive, view.nodes.live), reduced.matches);
      });
      previousLinks = links; wasLive = view.nodes.live;
      const ring = target.querySelector('.relay-fill')!, relayFill = view.nodes.relayLink;
      ring.setAttribute('stroke-dasharray', `${relayFill.total ? relayFill.done / relayFill.total : 0} 1`);
      target.querySelector('.connection-stage')!.classList.toggle('live', view.nodes.live);
      node('stage-failure').textContent = view.failure ? `${view.failure}${view.failureSinceMillis != null ? ` · ${clockTime(view.failureSinceMillis)}` : ''}` : '';
      for (const [id, action] of [['stage-primary', permitted(view.primary)], ['stage-secondary', permitted(view.secondary)]] as const) { const button = node<HTMLButtonElement>(id); button.hidden = !action; if (action) button.textContent = label(action); button.dataset.action = action?.type === 'Step' ? action.action : action?.type ?? ''; if (id === 'stage-primary') button.style.width = action?.type === 'Connect' ? '100%' : '94%'; button.disabled = !!flags.busy; }
      updateTimers();
      node('stage-notice').hidden = !view.notice; const noticeText = view.notice ? t(view.notice === 'CHOOSE_CAR' ? 'stage_notice_car' : view.notice === 'DISPLAY_CHANGED' ? 'stage_notice_display' : view.notice === 'TRUSTED_HTTPS' ? 'car.zeekrHttps' : 'setup.title') : '';
      patchMarkup(node('stage-notice'), view.notice ? `${icon(view.notice === 'CHOOSE_CAR' ? 'car-front' : view.notice === 'DISPLAY_CHANGED' ? 'refresh-cw' : 'settings')}${esc(noticeText)}${icon('chevron-left')}` : '');
      node('action-result').textContent = error ? t(error) : '';
      updateSegments(); if (drawer) drawDetails(); if (stepDialog) drawStep();
      timerVisible = view.countdown || view.headlineDeadlineMillis != null || view.supporting?.type === 'LiveTimer' && view.supporting.sinceMillis != null || view.supporting?.type === 'Elapsed';

    },
    tick: updateTimers, get timerVisible() { return timerVisible; },
    destroy() { reduced.removeEventListener('change', snapMotion); nudge?.cancel(); if (drawer) closeDrawer(); },
  };
}
