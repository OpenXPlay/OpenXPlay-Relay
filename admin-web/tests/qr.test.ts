// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-27

import { expect, it } from 'vitest';
import { createHash } from 'node:crypto';
import { qrMatrix, qrSvg } from '../src/qr';
it('matches an independently generated ZXing version 6-L, mask 0 receipt vector', () => {
  const matrix = qrMatrix('https://logs.openxplay.org/receipts/DEMO-20261027')!;
  expect(matrix).toHaveLength(41);
  const rows = matrix.map(row => row.map(Number).join('') + '\n').join('');
  expect(createHash('sha256').update(rows).digest('hex')).toBe('f7c08be52a27956125f4371c82fce1a68ec680b21bb6543f9317bcf708dee174');
});
it('enforces the byte capacity and renders a quiet zone with accessible text', () => {
  expect(qrMatrix('x'.repeat(134))).not.toBeNull(); expect(qrMatrix('x'.repeat(135))).toBeNull(); expect(qrMatrix('中'.repeat(45))).toBeNull();
  expect(qrSvg('receipt', 'Receipt QR')).toContain('viewBox="0 0 49 49" role="img" aria-label="Receipt QR"');
  expect(qrSvg('x'.repeat(135), 'Receipt QR')).toBeNull();
});
