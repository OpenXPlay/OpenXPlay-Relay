// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-27

import type { Settings } from './schema';
import type { Status } from './api';
import { carSelectionErrors } from './car-screen';
import { presentPath } from './path/present-path';
import { stepCommand } from './path/actions';
import { esc } from './html';
import { stepText, type Translate } from './stage';

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
export function preparationView(status: Status, t: Translate): string {
  const steps = presentPath(status.pathInput ?? {}).sections.filter(section => section.section === 'RELAY' || section.section === 'IPHONE').flatMap(section => section.steps).filter(step => !['CARPLAY_START', 'SESSION', 'PICTURE', 'IPHONE_BLUETOOTH', 'IPHONE_AUTH', 'IPHONE_WIFI'].includes(step.id));
  return `<p class="muted">${esc(t('setup_prepare_hint'))}</p>${steps.map(step => `<div class="inset-row" id="prepare-${step.id}" tabindex="-1"><span>${esc(t(step.title))}<small class="muted">${esc(stepText(step.detail, t))}</small></span><span>${esc(t(`stage.state.${step.state}`))}</span>${step.action && stepCommand(step.action, status.platform) ? `<button data-prepare-action="${step.id}">${esc(t(step.actionLabel ?? 'design_open'))}</button>` : ''}</div>`).join('')}<button id="setup-recheck">${esc(t('design_check_again'))}</button>`;
}

export function setupBlocker(step: SetupStep, settings: Settings, status: Status) {
  if (step === 'car') {
    const entry = Object.entries(carSelectionErrors(settings.values))[0];
    if (entry) return { id: 'field-' + entry[0], title: 'field.' + entry[0], reason: tReason(entry[1]) };
  }
  if (step === 'network' && setupBlocked(step, settings, status)) return { id: 'network-panel', title: 'menu.connection/network', reason: tReason('networks.hint') };
  if ((step === 'prepare' || step === 'start') && setupBlocked(step, settings, status)) {
    const path = presentPath(status.pathInput ?? {});
    const blocker = path.sections.flatMap(section => section.steps).find(row => row.state !== 'OK' && !['CARPLAY_START', 'SESSION', 'PICTURE'].includes(row.id));
    if (blocker) return { id: 'prepare-' + blocker.id, title: blocker.title, reason: blocker.detail ?? tReason(blocker.explanation ?? 'setup_prepare_hint') };
  }
  return null;
}
const tReason = (res: string) => ({ res });
