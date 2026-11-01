// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-31

import type { Status } from './api';
import { esc } from './html';

type RadioStatus = Pick<Status, 'platform' | 'network' | 'output'>;
export function carlinkRadioMissing(status: RadioStatus | null): boolean {
  return status?.platform === 'linux' && status.network.plan?.carlink.radioAvailable === false;
}
export function outputOptionDisabled(status: RadioStatus | null, value: unknown): boolean {
  return carlinkRadioMissing(status) && (value === 'carlink' || value === 'both');
}
export const browserOutput = { output: 'web' };
export function carlinkRadioBlocker(status: RadioStatus, t: (key: string) => string): string {
  if (!outputOptionDisabled(status, status.output)) return '';
  return `<article class="card" role="alert"><p>${esc(t('carlink.radioMissing'))}</p><button data-browser-output>${esc(t('carlink.useBrowser'))}</button></article>`;
}
