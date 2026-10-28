// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-27

import type { Status } from './api';
import type { Values } from './schema';
import { esc } from './html';
type Translate = (key: string, parameters?: Record<string, string | number>) => string;
export const teslaAddressKeys: Record<string, string> = {
  'virtual-name': 'teslaVirtualAddress', 'public-address': 'teslaPublicAddress',
  teslamirror: 'teslaMirrorAddress', 'tesla-android': 'teslaAndroidAddress',
};
export function teslaHint(values: Values, mode: string, t: Translate): string {
  if (mode === 'off') return '';
  return t(['virtual-name', 'teslamirror'].includes(mode) ? 'tesla.nameHint' : 'tesla.directHint', { address: String(values[teslaAddressKeys[mode]] ?? '') });
}
export function teslaNotes(values: Values, status: Status, t: Translate): string {
  if (!values.teslaBrowser || values.teslaBrowser === 'off') return '';
  const nameMode = ['virtual-name', 'teslamirror'].includes(String(values.teslaBrowser));
  const configured = values.publicNameEnabled === true && !!values.publicName && !!values.publicNameProvider && values.publicNameTokenSet === true && values.publicNameAgreeTerms === true;
  const note = (key: string, route: string, action: string) => `<p class="notice">${esc(t(key))} <a href="#settings/${route}">${esc(t(action))}</a></p>`;
  const tesla = status.pathInput?.car?.tesla;
  const needsBrowser = values.output === 'carlink' || tesla?.activation === 'NEEDS_BROWSER';
  const needsHotspot = values.networkMode === 'shared-wifi' || tesla?.activation === 'NEEDS_RELAY_HOTSPOT';
  return `<p class="muted">${esc(t('tesla.requirements'))}</p>${needsBrowser ? note('tesla.needsBrowser', 'output', 'tesla.openOutput') : ''}${needsHotspot ? note('tesla.needsRelayHotspot', 'connection/network', 'tesla.openNetwork') : ''}${nameMode && !configured ? note('tesla.needsName', 'access/public-name', 'tesla.openName') : ''}${tesla?.phase === 'FAILED' ? `<p class="error" role="alert">${esc(t('tesla.failed', { detail: tesla.code ?? '' }))}</p>` : ''}${tesla?.phase === 'READY' && tesla.name ? `<p><a href="${esc(tesla.name)}" target="_blank" rel="noopener">${esc(t('tesla.url', { url: tesla.name }))}</a></p>` : ''}`;
}
