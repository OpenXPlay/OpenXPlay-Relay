// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-15

import { expect, it } from 'vitest';
import { putSharedNetwork, deleteSharedNetwork, validateSharedNetworks, sharedNetworkUpdate } from '../src/shared-networks';
it('preserves ordered entries and omitted passwords, without returning metadata in updates', () => {
  const list = [{ ssid: 'Home', hasPassword: true, current: true }, { ssid: 'Car', hasPassword: false }];
  expect(sharedNetworkUpdate(putSharedNetwork(list, 'Home', 'Home'))).toEqual([{ ssid: 'Home' }, { ssid: 'Car' }]);
  expect(sharedNetworkUpdate(putSharedNetwork(list, 'Home', 'Home', ''))[0]).toEqual({ ssid: 'Home', password: '' });
  expect(deleteSharedNetwork(list, 'Home')).toEqual([list[1]]);
  expect(deleteSharedNetwork([list[0]], 'Home')).toEqual([]);
});
it('validates exact names, UTF-8 lengths, passwords and ten-entry limit', () => {
  expect(validateSharedNetworks([{ ssid: 'é'.repeat(16), password: '12345678' }])).toBe(true);
  for (const list of [[{ ssid: 'é'.repeat(17) }], [{ ssid: 'a', password: 'short' }], [{ ssid: 'a' }, { ssid: 'a' }], Array.from({ length: 11 }, (_, i) => ({ ssid: String(i) }))]) expect(validateSharedNetworks(list)).toBe(false);
  expect(validateSharedNetworks([{ ssid: 'A' }, { ssid: 'a' }])).toBe(true);
});
it('rejects duplicate edits and requires re-entry when renaming protected networks', () => {
  const list = [{ ssid: 'Home', hasPassword: true }, { ssid: 'Car' }];
  expect(() => putSharedNetwork(list, 'Home', 'Car', '12345678')).toThrow();
  expect(() => putSharedNetwork(list, 'Home', 'New')).toThrow();
  expect(putSharedNetwork(list, 'Home', 'New', '12345678').map(item => item.ssid)).toEqual(['New', 'Car']);
});

import { sharedNetworkPassword } from '../src/shared-networks';
it('requires a password unless open is explicit or an existing password is retained', () => {
  expect(sharedNetworkPassword(null, true, 'ignored')).toBe('');
  expect(sharedNetworkPassword({ ssid: 'Saved', hasPassword: true }, false, '')).toBeUndefined();
  expect(sharedNetworkPassword(null, false, '12345678')).toBe('12345678');
  for (const input of ['', 'short', 'a'.repeat(64), 'password\0']) expect(() => sharedNetworkPassword(null, false, input)).toThrow('error.networks');
  expect(() => sharedNetworkPassword({ ssid: 'Open', hasPassword: false }, false, '')).toThrow();
});
