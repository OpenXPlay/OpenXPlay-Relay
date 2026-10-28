// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-27

import type { RuntimeStartup, Status } from './api';
import type { ConnectionPath } from './path/model';

/** The runtime owns the five-second timer; browser polls never start a new countdown. */
export function arrivalCountdown(startup: RuntimeStartup | undefined, path: ConnectionPath, busy = false): boolean {
  return !!startup?.arrival.pending && startup.automatic && !startup.paused && path.canStart && !path.running && !busy;
}
export function arrivalRemaining(startup: RuntimeStartup | undefined, nowMillis: number, elapsedMillis: number): number {
  const arrival = startup?.arrival;
  const sampled = arrival?.remainingMillis ?? Math.max(0, (arrival?.deadlineMillis ?? nowMillis) - nowMillis);
  return Math.max(0, Math.min(5000, sampled - elapsedMillis));
}
export function restartPending(status: Status, local: boolean): boolean {
  // Linux reports the actual pending application, including after another admin client restarts.
  return status.pathInput?.platform === 'linux' ? status.notices.includes('notice.nextStart') : local || status.notices.includes('notice.nextStart');
}
