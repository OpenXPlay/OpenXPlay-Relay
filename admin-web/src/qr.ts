// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-27

/** QR Model 2, version 6-L, byte mode, mask 0. Longer targets use the server image. */
export function qrMatrix(text: string): boolean[][] | null {
  const bytes = new TextEncoder().encode(text); if (bytes.length > 134) return null;
  const bits: number[] = [], put = (value: number, count: number) => { for (let i = count - 1; i >= 0; i--) bits.push((value >>> i) & 1); };
  put(4, 4); put(bytes.length, 8); for (const byte of bytes) put(byte, 8);
  put(0, Math.min(4, 1088 - bits.length)); while (bits.length % 8) bits.push(0);
  const data: number[] = []; for (let i = 0; i < bits.length; i += 8) data.push(bits.slice(i, i + 8).reduce((v, bit) => (v << 1) | bit, 0));
  for (let pad = 0; data.length < 136; pad++) data.push(pad % 2 ? 0x11 : 0xec);
  function multiply(x: number, y: number) { let result = 0; for (let i = 7; i >= 0; i--) { result = (result << 1) ^ ((result >>> 7) * 0x11d); result ^= ((y >>> i) & 1) * x; } return result; }
  let divisor = [1], root = 1;
  for (let i = 0; i < 18; i++) { const next = Array(divisor.length + 1).fill(0); divisor.forEach((v, j) => { next[j] ^= v; next[j + 1] ^= multiply(v, root); }); divisor = next; root = multiply(root, 2); }
  function remainder(block: number[]) { const buffer = [...block, ...Array(18).fill(0)]; for (let i = 0; i < block.length; i++) { const factor = buffer[i]; divisor.forEach((v, j) => { buffer[i + j] ^= multiply(v, factor); }); } return buffer.slice(-18); }
  const blocks = [data.slice(0, 68), data.slice(68)], ecc = blocks.map(remainder), words: number[] = [];
  for (let i = 0; i < 68; i++) for (const b of blocks) words.push(b[i]); for (let i = 0; i < 18; i++) for (const b of ecc) words.push(b[i]);
  const size = 41, matrix = Array.from({ length: size }, () => Array<boolean>(size).fill(false)), reserved = matrix.map(row => row.slice());
  const set = (x: number, y: number, black: boolean) => { if (x >= 0 && y >= 0 && x < size && y < size) { matrix[y][x] = black; reserved[y][x] = true; } };
  for (const [x, y] of [[0, 0], [size - 7, 0], [0, size - 7]]) for (let dy = -1; dy <= 7; dy++) for (let dx = -1; dx <= 7; dx++) set(x + dx, y + dy, dx >= 0 && dx <= 6 && dy >= 0 && dy <= 6 && (dx === 0 || dx === 6 || dy === 0 || dy === 6 || dx >= 2 && dx <= 4 && dy >= 2 && dy <= 4));
  for (let i = 8; i < size - 8; i++) { set(6, i, i % 2 === 0); set(i, 6, i % 2 === 0); }
  for (let dy = -2; dy <= 2; dy++) for (let dx = -2; dx <= 2; dx++) set(34 + dx, 34 + dy, Math.max(Math.abs(dx), Math.abs(dy)) !== 1);
  const format = 0x77c4, bit = (i: number) => ((format >>> i) & 1) !== 0;
  for (let i = 0; i <= 5; i++) set(8, i, bit(i)); set(8, 7, bit(6)); set(8, 8, bit(7)); set(7, 8, bit(8));
  for (let i = 9; i < 15; i++) set(14 - i, 8, bit(i)); for (let i = 0; i < 8; i++) set(size - 1 - i, 8, bit(i)); for (let i = 8; i < 15; i++) set(8, size - 15 + i, bit(i)); set(8, size - 8, true);
  let index = 0;
  for (let right = size - 1; right >= 1; right -= 2) {
    if (right === 6) right = 5;
    for (let vertical = 0; vertical < size; vertical++) for (let j = 0; j < 2; j++) {
      const x = right - j, y = ((right + 1) & 2) === 0 ? size - 1 - vertical : vertical; if (reserved[y][x]) continue;
      const black = index < words.length * 8 && ((words[index >>> 3] >>> (7 - (index & 7))) & 1) !== 0;
      matrix[y][x] = black !== ((x + y) % 2 === 0); index++;
    }
  }
  return matrix;
}
export function qrSvg(text: string, label: string): string | null {
  const matrix = qrMatrix(text); if (!matrix) return null;
  const path = matrix.flatMap((row, y) => row.map((black, x) => black ? `M${x + 4},${y + 4}h1v1h-1z` : '')).join('');
  return `<svg class="receipt-qr" viewBox="0 0 49 49" role="img" aria-label="${label.replace(/[&"<>]/g, '')}" xmlns="http://www.w3.org/2000/svg"><rect width="49" height="49" fill="white"/><path d="${path}" fill="black"/></svg>`;
}
