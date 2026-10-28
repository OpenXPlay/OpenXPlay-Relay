// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-27

// Inspect names only; stream selected entries from the user's local File before uploading.
const MAX_ARCHIVE = 512 * 1024 * 1024, MAX_FILE = 16 * 1024;
const problem = (code: string): never => { throw new Error('identity_error_' + code); };
export function isIdentityZip(bytes: Uint8Array): boolean { return bytes[0] === 0x50 && bytes[1] === 0x4b && [[3, 4], [5, 6], [7, 8]].some(([a, b]) => bytes[2] === a && bytes[3] === b); }
export function encodeIdentityBytes(bytes: Uint8Array): string {
  let binary = '';
  for (let at = 0; at < bytes.length; at += 8192) binary += String.fromCharCode(...bytes.subarray(at, at + 8192));
  return btoa(binary);
}
async function read(file: Blob, start: number, size: number): Promise<Uint8Array> {
  if (start < 0 || size < 0 || start + size > file.size) problem('zip');
  return new Uint8Array(await file.slice(start, start + size).arrayBuffer());
}
interface Entry { name: Uint8Array; offset: number; compressed: number; expanded: number; flags: number; method: number; crc: number; boundary: number }
const view = (bytes: Uint8Array) => new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
async function names(file: Blob): Promise<Entry[]> {
  if (file.size > MAX_ARCHIVE) problem('limit');
  const tail = await read(file, Math.max(0, file.size - 65557), Math.min(file.size, 65557)), data = view(tail);
  let end = tail.length - 22;
  while (end >= 0 && !(data.getUint32(end, true) === 0x06054b50 && end + 22 + data.getUint16(end + 20, true) === tail.length)) end--;
  if (end < 0 || data.getUint32(end + 4, true) !== 0) problem('zip');
  const count = data.getUint16(end + 10, true), boundary = data.getUint32(end + 16, true);
  if (count > 20000) problem('limit');
  if (data.getUint16(end + 8, true) !== count || boundary + data.getUint32(end + 12, true) !== file.size - tail.length + end) problem('zip');
  let offset = boundary, expandedTotal = 0;
  const exact: Entry[][] = [[], []], fallback: Entry[][] = [[], []];
  for (let index = 0; index < count; index++) {
    const header = view(await read(file, offset, 46));
    if (header.getUint32(0, true) !== 0x02014b50) problem('zip');
    const flags = header.getUint16(8, true);
    if (flags & 65) problem('encrypted');
    const expanded = header.getUint32(24, true); expandedTotal += expanded;
    if (expandedTotal > MAX_ARCHIVE) problem('limit');
    const nameSize = header.getUint16(28, true), name = await read(file, offset + 46, nameSize);
    const basename = new TextDecoder().decode(name).split('/').at(-1)!.toLowerCase();
    const kind = basename.endsWith('.pk8') ? 0 : basename.endsWith('.p7b') ? 1 : -1;
    if (kind >= 0) {
      if (expanded > MAX_FILE) problem('size');
      const list = basename === 'identity.pk8' || basename === 'certificate.p7b' ? exact[kind] : fallback[kind];
      if (list.length < 2) list.push({ name, expanded, flags, method: header.getUint16(10, true), offset: header.getUint32(42, true), compressed: header.getUint32(20, true), crc: header.getUint32(16, true), boundary });
    }
    offset += 46 + nameSize + header.getUint16(30, true) + header.getUint16(32, true);
    if (offset > file.size - tail.length + end) problem('zip');
  }
  if (offset !== file.size - tail.length + end) problem('zip');
  const selected = exact.map((entries, kind) => entries.length ? entries : fallback[kind]);
  if (selected[0].length > 1) problem('duplicate_key');
  if (selected[1].length > 1) problem('duplicate_certificate');
  const result = selected.flat(); if (!result.length) problem('missing');
  return result;
}
async function extract(file: Blob, entry: Entry): Promise<string> {
  const header = view(await read(file, entry.offset, 30));
  if (header.getUint32(0, true) !== 0x04034b50 || header.getUint16(6, true) !== entry.flags || header.getUint16(8, true) !== entry.method) problem('zip');
  const nameSize = header.getUint16(26, true), name = await read(file, entry.offset + 30, nameSize);
  if (name.length !== entry.name.length || name.some((b, i) => b !== entry.name[i])) problem('zip');
  const start = entry.offset + 30 + nameSize + header.getUint16(28, true), finish = start + entry.compressed;
  if (finish > entry.boundary) problem('zip');
  const input = file.slice(start, finish).stream();
  let stream: ReadableStream<Uint8Array>;
  if (entry.method === 0) stream = input;
  else if (entry.method === 8) stream = input.pipeThrough(new DecompressionStream('deflate-raw'));
  else problem('zip');
  const reader = stream!.getReader(), bytes = new Uint8Array(MAX_FILE);
  let size = 0;
  try {
    for (;;) {
      const { done, value } = await reader.read(); if (done) break;
      if (size + value.length > MAX_FILE) { value.fill(0); problem('size'); }
      bytes.set(value, size); size += value.length; value.fill(0);
    }
    let crc = 0xffffffff;
    for (const byte of bytes.subarray(0, size)) { crc ^= byte; for (let bit = 0; bit < 8; bit++) crc = (crc >>> 1) ^ (0xedb88320 & -(crc & 1)); }
    if (size !== entry.expanded || ((crc ^ 0xffffffff) >>> 0) !== entry.crc) problem('zip');
    return encodeIdentityBytes(bytes.subarray(0, size));
  } catch (e) { if (e instanceof Error && e.message.startsWith('identity_error_')) throw e; return problem('zip'); }
  finally { bytes.fill(0); await reader.cancel().catch(() => {}); reader.releaseLock(); }
}
export async function readIdentitySelection(files: File[], followUp: boolean): Promise<string[]> {
  if (!files.length || files.length > (followUp ? 1 : 2)) problem('selection');
  const selected: string[] = [];
  for (const file of files) {
    const zip = isIdentityZip(await read(file, 0, Math.min(4, file.size)));
    if (zip) {
      if (files.length !== 1) problem('selection');
      const entries = await names(file);
      if (followUp && entries.length !== 1) problem('selection');
      for (const entry of entries) selected.push(await extract(file, entry));
    } else {
      if (!file.size || file.size > MAX_FILE) problem('size');
      const bytes = await read(file, 0, file.size);
      try { selected.push(encodeIdentityBytes(bytes)); } finally { bytes.fill(0); }
    }
  }
  return selected;
}
