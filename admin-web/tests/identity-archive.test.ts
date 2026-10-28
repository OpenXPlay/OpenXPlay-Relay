// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-27

import { expect, it } from 'vitest';
import { deflateRawSync } from 'node:zlib';
import { readIdentitySelection } from '../src/identity-archive';
function zip(entries: [string, Uint8Array][], method = 8): File {
  const chunks: Buffer[] = [], central: Buffer[] = []; let offset = 0;
  for (const [name, data] of entries) {
    let crc = 0xffffffff;
    for (const byte of data) { crc ^= byte; for (let n = 0; n < 8; n++) crc = (crc >>> 1) ^ (0xedb88320 & -(crc & 1)); }
    crc = (crc ^ 0xffffffff) >>> 0;
    const encoded = Buffer.from(name), body = method === 8 ? deflateRawSync(data) : Buffer.from(data), local = Buffer.alloc(30), directory = Buffer.alloc(46);
    local.writeUInt32LE(0x04034b50); local.writeUInt16LE(20, 4); local.writeUInt16LE(method, 8); local.writeUInt32LE(crc, 14); local.writeUInt32LE(body.length, 18); local.writeUInt32LE(data.length, 22); local.writeUInt16LE(encoded.length, 26);
    directory.writeUInt32LE(0x02014b50); directory.writeUInt16LE(20, 4); directory.writeUInt16LE(20, 6); directory.writeUInt16LE(method, 10); directory.writeUInt32LE(crc, 16); directory.writeUInt32LE(body.length, 20); directory.writeUInt32LE(data.length, 24); directory.writeUInt16LE(encoded.length, 28); directory.writeUInt32LE(offset, 42);
    chunks.push(local, encoded, body); central.push(directory, encoded); offset += local.length + encoded.length + body.length;
  }
  const end = Buffer.alloc(22), directory = Buffer.concat(central); end.writeUInt32LE(0x06054b50); end.writeUInt16LE(entries.length, 8); end.writeUInt16LE(entries.length, 10); end.writeUInt32LE(directory.length, 12); end.writeUInt32LE(offset, 16);
  return new File([Buffer.concat([...chunks, directory, end])], 'unrelated.apk');
}
const key = Uint8Array.from([1, 2, 3]), cert = Uint8Array.from([4, 5, 6]);
it.each([0, 8])('reads a signature-detected ZIP with nested case-insensitive names (method %s)', async method => {
  const selected = await readIdentitySelection([zip([['path/IDENTITY.PK8', key], ['CERTIFICATE.P7B', cert], ['ignored', new Uint8Array(1024 * 1024)]], method)], false);
  expect(selected).toEqual(['AQID', 'BAUG']);
});
it('keeps plain selections in either order and lets a one-file archive complete a follow-up', async () => {
  expect(await readIdentitySelection([new File([cert], 'a'), new File([key], 'b')], false)).toEqual(['BAUG', 'AQID']);
  expect(await readIdentitySelection([zip([['a.pk8', key]])], true)).toEqual(['AQID']);
});
it('prefers exact names and otherwise requires unique extensions', async () => {
  expect(await readIdentitySelection([zip([['a.pk8', cert], ['identity.pk8', key]])], false)).toEqual(['AQID']);
  await expect(readIdentitySelection([zip([['a.pk8', key], ['b.pk8', key]])], false)).rejects.toThrow('identity_error_duplicate_key');
  await expect(readIdentitySelection([zip([['certificate.p7b', cert], ['nested/CERTIFICATE.P7B', cert]])], false)).rejects.toThrow('identity_error_duplicate_certificate');
});
it('never searches unrelated contents and enforces follow-up and selection rules', async () => {
  await expect(readIdentitySelection([zip([['unrelated', key]])], false)).rejects.toThrow('identity_error_missing');
  const complete = zip([['a.pk8', key], ['b.p7b', cert]]);
  await expect(readIdentitySelection([complete], true)).rejects.toThrow('identity_error_selection');
  await expect(readIdentitySelection([complete, new File([key], 'key')], false)).rejects.toThrow('identity_error_selection');
});
it('rejects oversized entries and corrupted CRCs before uploading', async () => {
  await expect(readIdentitySelection([zip([['a.pk8', new Uint8Array(16385)]])], false)).rejects.toThrow('identity_error_size');
  const bytes = new Uint8Array(await zip([['identity.pk8', key]]).arrayBuffer());
  const at = bytes.findIndex((_, i) => bytes[i] === 0x50 && bytes[i + 1] === 0x4b && bytes[i + 2] === 1 && bytes[i + 3] === 2); bytes[at + 16] ^= 1;
  await expect(readIdentitySelection([new File([bytes], 'bad')], false)).rejects.toThrow('identity_error_zip');
});
it('rejects encrypted archives before attempting to read material', async () => {
  const bytes = new Uint8Array(await zip([['identity.pk8', key]]).arrayBuffer());
  const at = bytes.findIndex((_, i) => bytes[i] === 0x50 && bytes[i + 1] === 0x4b && bytes[i + 2] === 1 && bytes[i + 3] === 2); bytes[at + 8] = 1;
  await expect(readIdentitySelection([new File([bytes], 'encrypted')], false)).rejects.toThrow('identity_error_encrypted');
});
