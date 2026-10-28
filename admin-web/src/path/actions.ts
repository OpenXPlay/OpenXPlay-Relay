// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-27

import type { StepAction } from './model';
import type { StageAction } from './present-stage';
export type Command = { kind: 'api'; endpoint: 'actions/start' | 'actions/stop' | 'actions/restart' | 'actions/pair-iphone' | 'actions/enable-bluetooth' | 'actions/enable-wifi' | 'carlink/connect' } | { kind: 'route'; route: string } | { kind: 'phone' | 'pin' | 'network' | 'hotspot' | 'copy' | 'cancel' };
export function stepCommand(action: StepAction, platform: 'linux' | 'android'): Command | null {
  switch (action) {
    case 'TURN_ON_BLUETOOTH': return platform === 'linux' ? { kind: 'api', endpoint: 'actions/enable-bluetooth' } : null;
    case 'OPEN_WIFI': return platform === 'linux' ? { kind: 'api', endpoint: 'actions/enable-wifi' } : null;
    case 'FIX_SETTINGS': return { kind: 'route', route: 'settings' };
    case 'START_NOW': return { kind: 'api', endpoint: 'actions/start' };
    case 'RETRY': return { kind: 'api', endpoint: 'actions/restart' };
    case 'PAIR_IPHONE': return { kind: 'api', endpoint: 'actions/pair-iphone' };
    case 'CHOOSE_IPHONE': return { kind: 'phone' };
    case 'ENTER_PIN': return { kind: 'pin' };
    case 'RECONNECT_CARLINK': return { kind: 'api', endpoint: 'carlink/connect' };
    case 'CHOOSE_SHARED_WIFI': return { kind: 'network' };
    case 'OPEN_HOTSPOT': return platform === 'linux' ? { kind: 'hotspot' } : null;
    case 'COPY_PAGE_ADDRESS': return { kind: 'copy' };
    default: return null;
  }
}
export function stageCommand(action: StageAction, platform: 'linux' | 'android', running: boolean): Command | null {
  if (action.type === 'Step') return stepCommand(action.action, platform);
  if (action.type === 'NotNow') return { kind: 'cancel' };
  if (action.type === 'Connect') return running ? null : { kind: 'api', endpoint: 'actions/start' };
  return running ? { kind: 'api', endpoint: 'actions/stop' } : null;
}
