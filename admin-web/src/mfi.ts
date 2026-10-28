// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-27

import type { Values } from './schema';

export function mfiSubmission(values: Values, key: string, value: Values[string]): Values | null {
  if (!['mfiTarget', 'mfiI2cPath', 'mfiRemoteServer', 'mfiRemoteToken'].includes(key)) return null;
  if (key === 'mfiRemoteToken') return { mfiRemoteToken: value };
  return { mfiTarget: values.mfiTarget, mfiI2cPath: values.mfiI2cPath, mfiRemoteServer: values.mfiRemoteServer, [key]: value };
}
export function mfiError(values: Values): string | null {
  if (values.mfiTarget === 'i2c' && !/^\/dev\/i2c-\d+$/.test(String(values.mfiI2cPath))) return 'mfi.enterPath';
  if (values.mfiTarget === 'remote') {
    try {
      const url = new URL(String(values.mfiRemoteServer).trim());
      if (!['http:', 'https:'].includes(url.protocol) || !url.hostname || url.username || url.password || url.hash) return 'mfi.enterServer';
    } catch { return 'mfi.enterServer'; }
  }
  return null;
}
export function mfiReadiness(values: Values, t: (key: string) => string): string {
  const status = String(values.mfiSourceStatus ?? 'missing');
  return t(`mfi.readiness.${['ready', 'missing', 'invalid', 'permission', 'unsupported'].includes(status) ? status : 'invalid'}`);
}
