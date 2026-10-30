// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-27

import { afterEach, describe, expect, it, vi } from 'vitest';
import { IdentitySelection, identityApi, identityMarkup, filePem, materialKey, type IdentityStatus } from '../src/identity';
import { normalizeRoute, parentRoute, pages, pageAvailable, menuSummary } from '../src/settings-tree';
import { stepCommand } from '../src/path/actions';
import { translate } from '../src/i18n';
import { mockSettings, linuxFixtureStatus } from '../dev-server';
const t = (key: string) => translate('en', key);
const status: IdentityStatus = { mfi: { state: 'MISSING', fingerprint: null }, tls: { state: 'READY', fingerprint: 'AA:BB', imported: false } };
afterEach(() => vi.unstubAllGlobals());
describe('identity settings', () => {
  it('is top-level with old routes redirected and stage import routed to it', () => {
    expect(pages.find(([p]) => p === 'identity')).toBeTruthy();
    for (const alias of ['settings/developer/identity', 'developer/identity', 'settings:identity']) expect(normalizeRoute('#' + alias)).toBe('settings/identity');
    expect(parentRoute('settings/identity')).toBe('settings');
    expect(stepCommand('IMPORT_IDENTITY', 'linux')).toEqual({ kind: 'route', route: 'settings/identity' });
    expect(pageAvailable(mockSettings(true), { ...linuxFixtureStatus('ready'), adminFeatures: ['identity'] }, 'settings/identity')).toBe(true);
  });
  it('shows the current installed, invalid or missing summary without diagnostic polling', () => {
    for (const state of ['READY', 'INVALID', 'MISSING'] as const) {
      const current = { ...linuxFixtureStatus('ready'), identity: { ...status, mfi: { state, fingerprint: null } } };
      expect(menuSummary(mockSettings(true), current, 'settings/identity', 'en', t)).toBe(t(materialKey(state)));
    }
  });
  it('opens CarPlay first, keeps TLS collapsed and explains automatic TLS removal', () => {
    const markup = identityMarkup(status, new IdentitySelection(), false, '', t);
    expect(markup.indexOf('CarPlay identity')).toBeLessThan(markup.indexOf('TLS certificate'));
    expect(markup).toContain('id="carplay-identity" open');
    expect(markup).toContain(t('identity_tls_automatic'));
    expect(markup).not.toContain(t('direlay_installed'));
    expect(markup).toContain('class="button pill"');
    expect(markup).toContain('data-file-name="identity-files"');
    expect(markup).toContain('AA:BB'); expect(markup).toMatch(/id="tls-remove" disabled/);
    expect(markup).toContain('type="password"'); expect(markup).not.toContain('text-overflow');
  });
  it.each(['key', 'certificate', 'unknown'] as const)('asks for the missing part after %s and clears it on success', recognized => {
    const selection = new IdentitySelection(); selection.accept({ ok: true, followUp: 'relay-owned-token', recognized });
    expect(identityMarkup(status, selection, false, '', t)).toContain(t('identity_follow_' + recognized));
    expect(identityMarkup(status, selection, false, '', t)).not.toMatch(/id="identity-files"[^>]*multiple/);
    selection.accept({ ok: true, fingerprint: 'FF' }); expect(selection.followUp).toBeUndefined();
  });
  it('escapes fingerprints and messages and reports pending receiver application', () => {
    const current = { ...status, tls: { ...status.tls, fingerprint: '<img>', pending: true } };
    const markup = identityMarkup(current, new IdentitySelection(), false, 'identity_error_pair', t);
    expect(markup).not.toContain('<img>'); expect(markup).toContain('&lt;img&gt;'); expect(markup).toContain(t('identity_tls_pending')); expect(markup).toContain(t('identity_error_pair'));
  });
  it('has Android wording in English and Chinese for every import outcome', () => {
    for (const language of ['en', 'zh-CN'] as const) for (const key of ['identity_error_pair', 'identity_error_encrypted', 'identity_error_limit', 'identity_error_zip', 'identity_error_tls', 'identity_error_pkcs12_unavailable', 'identity_error_pem', 'identity_import_pem', 'identity_pem_chain', 'identity_pem_key', 'direlay_identity_hint', 'dev_remove_identity_question', 'dev_remove_tls_question', 'menu.identity']) expect(translate(language, key)).not.toBe(key);
  });
  it('sends explicit confirmation and never retries a material mutation', async () => {
    const fetch = vi.fn().mockResolvedValue({ ok: true, json: async () => ({ ok: true }), headers: new Headers() }); vi.stubGlobal('fetch', fetch);
    await identityApi.select(['base64'], 'follow'); await identityApi.remove('identity'); await identityApi.remove('tls'); await identityApi.tls('p12', 'private-password'); await identityApi.cancel('follow'); await identityApi.pem('chain', 'key');
    expect(fetch.mock.calls.map(([url]) => url)).toEqual(['/api/v0/identity/import', '/api/v0/identity', '/api/v0/tls', '/api/v0/tls/import', '/api/v0/identity/import', '/api/v0/tls/import']);
    expect(JSON.parse(fetch.mock.calls[5][1].body)).toEqual({ chainPem: 'chain', keyPem: 'key' });
    expect(JSON.parse(fetch.mock.calls[1][1].body)).toEqual({ confirm: true });
    expect(JSON.parse(fetch.mock.calls[0][1].body)).toEqual({ files: ['base64'], followUp: 'follow' });
  });
});

it.each(['en', 'zh-CN'] as const)('offers PEM when the optional PKCS12 decoder is absent (%s)', language => {
  const t = (key: string) => translate(language, key);
  const markup = identityMarkup(status, new IdentitySelection(), false, 'identity_error_pkcs12_unavailable', t);
  for (const id of ['tls-pem-chain', 'tls-pem-key', 'tls-pem-import']) expect(markup).toContain(`id="${id}"`);
  expect(t('identity_error_pkcs12_unavailable')).toContain('PEM');
  expect(markup).toContain(t('identity_error_pkcs12_unavailable'));
});
it('bounds PEM files, rejects invalid UTF-8 and clears the loaded bytes', async () => {
  const bytes = new TextEncoder().encode('certificate');
  const file = { size: bytes.length, arrayBuffer: async () => bytes.buffer } as File;
  expect(await filePem(file, 1024)).toBe('certificate'); expect([...bytes].every(byte => byte === 0)).toBe(true);
  await expect(filePem({ size: 1025 } as File, 1024)).rejects.toThrow('identity_error_pem');
  await expect(filePem({ size: 1, arrayBuffer: async () => new Uint8Array([255]).buffer } as File, 1024)).rejects.toThrow('identity_error_pem');
});

it('escapes selected file names beside the single file-choice pill', () => {
  const markup = identityMarkup(status, new IdentitySelection(), false, '', t, { 'identity-files': '<key.pem>, certificate.p7b' });
  expect(markup).toContain('&lt;key.pem&gt;, certificate.p7b');
  expect(markup).not.toContain('<key.pem>');
  expect(markup.match(/id="identity-files"/g)).toHaveLength(1);
});
it('focuses CarPlay once, preserves the expanded TLS form and clears stale selected names after import', async () => {
  const { mountIdentity } = await import('../src/identity');
  const focused: string[] = [];
  class Host extends EventTarget {
    textContent = ''; value = ''; open = false; files: File[] = []; dataset: Record<string, string> = {};
    markup = ''; nodes = new Map<string, Host>(); id = '';
    get innerHTML() { return this.markup; }
    set innerHTML(value: string) { this.markup = value; this.nodes.clear(); }
    querySelector(selector: string): Host | null {
      if (!this.markup) return null;
      if (!this.nodes.has(selector)) { const node = new Host(); node.id = selector.slice(1); this.nodes.set(selector, node); }
      return this.nodes.get(selector)!;
    }
    querySelectorAll(selector: string) { return selector === 'input[type="file"]' ? ['identity-files', 'tls-file', 'tls-pem-chain', 'tls-pem-key'].map(id => this.querySelector('#' + id)!) : []; }
    focus() { focused.push(this.id); } scrollIntoView() {}
  }
  vi.spyOn(identityApi, 'status').mockResolvedValue(status);
  const importTls = vi.spyOn(identityApi, 'tls').mockResolvedValue(status);
  const root = new Host();
  const dispose = mountIdentity(root as unknown as HTMLElement, new IdentitySelection(), t, () => 'error.request', async () => {});
  try {
    await vi.waitFor(() => expect(focused).toEqual(['carplay-identity summary']));
    root.querySelector('#identity-tls')!.open = true;
    const file = root.querySelector('#tls-file')!;
    file.files = [{ name: 'selected.p12', size: 1, arrayBuffer: async () => new Uint8Array([1]).buffer } as File];
    file.dispatchEvent(new Event('change'));
    expect(root.querySelector('[data-file-name="tls-file"]')!.textContent).toBe('selected.p12');
    root.querySelector('#tls-password')!.value = 'test-password';
    root.querySelector('#tls-import')!.dispatchEvent(new Event('click'));
    await vi.waitFor(() => expect(importTls).toHaveBeenCalledWith('AQ==', 'test-password'));
    await vi.waitFor(() => expect(root.innerHTML).not.toContain('selected.p12'));
    expect(root.querySelector('#identity-tls')!.open).toBe(true);
    expect(focused).toHaveLength(1);
  } finally { dispose(); vi.restoreAllMocks(); }
});
