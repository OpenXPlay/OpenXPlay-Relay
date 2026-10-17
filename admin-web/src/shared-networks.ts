// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-15

import type { SavedNetwork } from './schema';
export function validateSharedNetworks(value: unknown): value is SavedNetwork[] {
  return Array.isArray(value) && value.length <= 10 && new Set(value.map(item => item?.ssid)).size === value.length && value.every(item =>
    item && typeof item.ssid === 'string' && item.ssid.trim().length > 0 && new TextEncoder().encode(item.ssid).length <= 32 && !item.ssid.includes('\0') &&
    (item.password === undefined || (typeof item.password === 'string' && !item.password.includes('\0') && (item.password.length === 0 || (item.password.length >= 8 && item.password.length <= 63)))));
}
export function putSharedNetwork(list: SavedNetwork[], original: string | null, ssid: string, password?: string): SavedNetwork[] {
  if (list.some(item => item.ssid === ssid && item.ssid !== original)) throw new Error('error.networks');
  const previous = list.find(item => item.ssid === original);
  // Renaming creates a new SSID, so its existing password cannot be retained by the server.
  if (previous?.hasPassword && original !== ssid && password === undefined) throw new Error('error.networks');
  const entry: SavedNetwork = { ssid, ...(previous?.ssid === ssid ? previous : {}), ...(password === undefined ? {} : { password, hasPassword: password.length > 0 }) };
  const next = original === null ? [...list, entry] : list.map(item => item.ssid === original ? entry : item);
  if (!validateSharedNetworks(next)) throw new Error('error.networks');
  return next;
}
export function deleteSharedNetwork(list: SavedNetwork[], ssid: string): SavedNetwork[] { return list.filter(item => item.ssid !== ssid); }
export function sharedNetworkUpdate(list: SavedNetwork[]): SavedNetwork[] {
  return list.map(({ ssid, password }) => ({ ssid, ...(password === undefined ? {} : { password }) }));
}

export function sharedNetworkPassword(entry: SavedNetwork | null, open: boolean, input: string): string | undefined {
  if (open) return '';
  if (entry?.hasPassword && !input) return undefined;
  if (!validateSharedNetworks([{ ssid: entry?.ssid ?? 'Wi-Fi', password: input }]) || !input) throw new Error('error.networks');
  return input;
}
