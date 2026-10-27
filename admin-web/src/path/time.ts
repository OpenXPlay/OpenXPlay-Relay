// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-27

import { clockTime } from './present-path';
export function countdownSeconds(deadlineMillis: number, nowMillis: number): number { return Math.max(0, Math.ceil((deadlineMillis - nowMillis) / 1000)); }
export function formatDiagnostic(line: string, version: string, nowMillis: number, zone?: string): string { return `${line} | ${version} ${clockTime(nowMillis, zone)}`; }
