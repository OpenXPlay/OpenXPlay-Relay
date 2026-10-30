// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-27

import type { Settings } from './schema';
import type { Status } from './api';
import { carSelectionErrors } from './car-screen';
import { presentPath } from './path/present-path';
import { stepCommand } from './path/actions';
import { pairingView, relayBluetoothName } from './pairing';
import { esc } from './html';
import { stepText, type Translate } from './stage';
import type { PathStep, StepText } from './path/model';

export const setupSteps = ['car', 'output', 'connection', 'network', 'prepare', 'start'] as const;
export type SetupStep = typeof setupSteps[number];
export function setupFields(step: SetupStep): string[] {
  return { car: ['carBrand', 'carBrandText', 'carModel', 'carModelText', 'carYear', 'carYearText'], output: ['output', 'carlinkName'], connection: ['iphone'], network: ['networkMode'], prepare: [], start: ['autoStart', 'bootStart'] }[step];
}
export function setupBlocked(step: SetupStep, settings: Settings, status: Status): boolean {
  if (step === 'car') return Object.keys(carSelectionErrors(settings.values)).length > 0;
  if (step === 'network') return settings.values.networkMode === 'shared-wifi' && (!Array.isArray(settings.values.sharedWifiNetworks) || !settings.values.sharedWifiNetworks.length);
  if (step === 'prepare' || step === 'start') return !status.running && !presentPath(status.pathInput ?? {}).canStart;
  return false;
}
export function guideBack(step: SetupStep): string {
  const index = setupSteps.indexOf(step);
  return index > 0 ? 'setup/' + setupSteps[index - 1] : 'home';
}
// The relay and iPhone steps decide whether CarPlay can start; the car steps follow once it runs.
function preparationSteps(status: Status): PathStep[] {
  return presentPath(status.pathInput ?? {}).sections.filter(section => section.section === 'RELAY' || section.section === 'IPHONE').flatMap(section => section.steps).filter(step => !['CARPLAY_START', 'SESSION', 'PICTURE', 'IPHONE_BLUETOOTH', 'IPHONE_AUTH', 'IPHONE_WIFI'].includes(step.id));
}
/**
 * Only the first unfinished step stays open, with its one action; the rest fold under a ready count.
 * marked keeps that step flagged after a refused Next, across refreshes. compact shows only it (start step).
 */
export function preparationView(status: Status, t: Translate, options: { marked?: boolean; compact?: boolean } = {}): string {
  const steps = preparationSteps(status);
  const blocker = steps.find(step => step.state !== 'OK'), rest = steps.filter(step => step !== blocker);
  const pair = pairingView(status);
  const row = (step: PathStep, marked = false) => {
    const pairing = step.id === 'IPHONE_PAIRED' && pair.open;
    const detail = pairing ? t('home.pairCountdown', { name: relayBluetoothName(status), time: pair.time }) : stepText(step.detail, t) || (step.explanation ? t(step.explanation) : '');
    const action = pairing ? pair.cancelable ? `<button class="quiet" data-prepare-cancel>${esc(t('pairing.cancel'))}</button>` : ''
      : step.action && stepCommand(step.action, status.platform) ? `<button class="quiet" data-prepare-action="${step.id}">${esc(t(step.actionLabel ?? 'design_open'))}</button>`
      : step.state !== 'OK' && step !== blocker ? `<span class="muted">${esc(t(`stage.state.${step.state}`))}</span>` : '';
    return `<div class="inset-row prepare-row${marked ? ' guide-blocker' : ''}" id="prepare-${step.id}" tabindex="-1"><span class="row-copy"><span>${esc(t(step.title))}</span>${detail ? `<small class="muted" ${pairing ? 'id="guide-pairing-remaining" role="timer"' : ''}>${esc(detail)}</small>` : ''}</span>${action}</div>`;
  };
  const open = blocker ? `<div class="inset-group">${row(blocker, options.marked)}</div>` : '';
  if (options.compact) return open;
  const ready = steps.filter(step => step.state === 'OK').length;
  return `<p class="muted">${esc(t('setup_prepare_hint'))}</p>${open}${rest.length ? `<details class="inset-group" data-key="prepare-ready"><summary>${esc(t('setup.readyCount', { ready, total: steps.length }))}</summary>${rest.map(step => row(step)).join('')}</details>` : ''}<button class="quiet" id="setup-recheck">${esc(t('design_check_again'))}</button>`;
}
export interface SetupBlocker { row: string; field?: string; title: string; reason: StepText }
/** Names what keeps Next from advancing: a field to fill in, or the preparation step that is not ready. */
export function setupBlocker(step: SetupStep, settings: Settings, status: Status): SetupBlocker | null {
  if (step === 'car') {
    const entry = Object.entries(carSelectionErrors(settings.values))[0];
    if (entry) { const field = entry[0] + (settings.values[entry[0]] === 'other' ? 'Text' : ''); return { row: 'field-' + field, field, title: 'field.' + entry[0], reason: { res: entry[1] } }; }
  }
  if (step === 'network' && setupBlocked(step, settings, status)) return { row: 'setup-blocker-network', title: 'menu.connection/network', reason: { res: 'networks.hint' } };
  if ((step === 'prepare' || step === 'start') && setupBlocked(step, settings, status)) {
    const blocker = preparationSteps(status).find(row => row.state !== 'OK');
    if (blocker) return { row: 'prepare-' + blocker.id, title: blocker.title, reason: blocker.detail ?? { res: blocker.explanation ?? 'setup_prepare_hint' } };
  }
  return null;
}
