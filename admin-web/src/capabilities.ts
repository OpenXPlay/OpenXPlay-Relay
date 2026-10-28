// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-27

import type { Status } from './api';
import type { Translate } from './stage';
import { esc } from './html';

export function capabilityFieldVisible(key: string, status: Status): boolean {
  return !['webTranscode', 'carlinkHevc', 'carlinkPictureSize'].includes(key) || capabilityFieldEnabled(key, status);
}
export function capabilityFieldEnabled(key: string, status: Status): boolean {
  if (status.platform !== 'linux') return true;
  if (key === 'playCarPlayAudio') return status.localAudio?.playbackAvailable === true;
  if (['microphoneSource', 'microphoneSourceEnabled'].includes(key)) return status.localAudio?.captureAvailable === true;
  const output = key === 'webTranscode' ? status.transcoding?.web : status.transcoding?.clink;
  if (['webTranscode', 'carlinkHevc', 'carlinkPictureSize'].includes(key)) {
    if (!output?.available) return false;
    if (key === 'webTranscode') return output.decode.some(codec => ['H264', 'H265'].includes(codec)) && output.encode.some(e => e.codec === 'H264');
    if (!output.decode.includes('H264')) return false;
    if (key === 'carlinkHevc') return output.encode.some(e => e.codec === 'H265');
    return key !== 'carlinkPictureSize' || output.scaling && output.encode.some(e => e.codec === 'H264');
  }
  return true;
}
export function capabilityView(status: Status, t: Translate): string {
  if (status.platform !== 'linux') return '';
  const rows = (['web', 'clink'] as const).map(key => {
    const cap = status.transcoding?.[key];
    return `<dt>${esc(key === 'web' ? t('option.output.web') : 'Carlink')}</dt><dd>${esc(t(cap?.available ? 'capability.available' : 'capability.unavailable'))}${cap?.description ? ` · ${esc(cap.description)}` : ''}</dd>`;
  }).join('');
  return `<section class="inset-group"><h3>${esc(t('capability.encoder'))}</h3><dl class="facts">${rows}</dl><pre>${esc(JSON.stringify(status.transcoding ?? {}, null, 2))}</pre><h3>${esc(t('capability.localAudio'))}</h3><pre>${esc(JSON.stringify(status.localAudio ?? {}, null, 2))}</pre></section>`;
}
