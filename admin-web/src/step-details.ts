// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-27

import { esc } from './html';
import type { PathStep } from './path/model';
import { stepCommand } from './path/actions';
import { stepText, type Translate } from './stage';

export function stepTrail(step: PathStep, addresses: string[]): string[] {
  return [...new Set([...(step.id === 'CAR_PAGE' ? addresses : []), ...step.trail].filter(value => value.trim()))];
}
export function stepDetailsView(step: PathStep, addresses: string[], platform: 'android' | 'linux', t: Translate): string {
  const evidence = (value: string) => `<div class="address-row"><span class="mono">${esc(value)}</span><button data-evidence="${esc(value)}" aria-label="${esc(t('ui.copyValue'))}: ${esc(value)}">${esc(t('ui.copyValue'))}</button></div>`;
  const trail = stepTrail(step, addresses);
  return `<h2>${esc(t(step.title))}</h2><p>${esc(stepText(step.detail, t))}</p>${step.explanation ? `<p class="muted">${esc(t(step.explanation))}</p>` : ''}${step.evidence ? evidence(stepText(step.evidence, t)) : ''}${step.code ? evidence(step.code) : ''}${step.sinceMillis != null ? `<time class="mono" datetime="${new Date(step.sinceMillis).toISOString()}">${esc(new Date(step.sinceMillis).toLocaleString())}</time>` : ''}${trail.length ? `<h3>${esc(t('design_trail'))}</h3><pre>${esc(trail.join('\n'))}</pre>${evidence(trail.join('\n'))}` : ''}<div class="dialog-actions">${step.action && stepCommand(step.action, platform) ? `<button class="primary" data-action>${esc(t(step.actionLabel ?? 'design_open'))}</button>` : ''}<button data-recheck>${esc(t('design_check_again'))}</button><button data-close>${esc(t('ui.close'))}</button></div><p data-recheck-result role="status"></p>`;
}
