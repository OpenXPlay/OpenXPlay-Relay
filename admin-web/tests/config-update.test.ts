// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-27

import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { api, ApiError } from '../src/api';
import { configText, configErrorText, mountFeaturePage } from '../src/feature-pages';
import { translate } from '../src/i18n';
import { UpdateState, updates, updateMenuVersion, updateFailureText, localizedNotes, releaseMarkup, type UpdateCheck } from '../src/update-check';
vi.mock('../src/dom', async original => ({ ...await original<typeof import('../src/dom')>(), patchMarkup: (node: Host, markup: string) => { node.innerHTML = markup; } }));
const t = (key: string, parameters?: Record<string, string | number>) => translate('en', key, parameters);
const zh = (key: string, parameters?: Record<string, string | number>) => translate('zh-CN', key, parameters);
const candidate = { latest: '0.1.7', outcome: 'available' as const, url: 'https://github.com/OpenXPlay/OpenXPlay-Relay/releases/tag/linux-v0.1.7', notes: 'Improved connection · 连接改进\n\n**Changes**\n- Faster `Carlink`\n\n**更新**\n- 更快的 **Carlink**\n---\nprivate provenance', publishedAt: '2026-10-27T12:00:00Z' };
const result: UpdateCheck = { ...candidate, current: '0.1.6', channel: 'stable', checkedMillis: 100, candidates: { stable: candidate, beta: { ...candidate, latest: '0.1.8-beta.1' } } };
beforeEach(() => { updates.channel = 'stable'; updates.result = null; updates.failure = null; updates.checking = false; });
afterEach(() => { vi.restoreAllMocks(); vi.unstubAllGlobals(); });
it('retains original config bytes including duplicate-key evidence and rejects malformed UTF-8', async () => {
  const text = '{"format":"openxplay-user-config","version":1,"settings":{"frameRate":"30","frameRate":"60"}}';
  const original = configText(new TextEncoder().encode(text).buffer);
  const fetcher = vi.fn().mockResolvedValue(new Response('{}')); vi.stubGlobal('fetch', fetcher);
  await api.configImport(original, true);
  expect(fetcher.mock.calls[0][1].body).toBe(text);
  expect(() => configText(new Uint8Array([123, 34, 0xff, 34, 58, 49, 125]).buffer)).toThrow('files.notJson');
  expect(() => configText(new Uint8Array(65537).buffer)).toThrow('files.tooLarge');
  expect(() => configText(new TextEncoder().encode('\ufeff{}').buffer)).toThrow('files.notJson');
});
it.each([t, zh])('shows strict envelope errors and every per-key API message', tr => {
  for (const [code, key] of [['not-json', 'files.notJson'], ['bad-version', 'files.badVersion'], ['too-large', 'files.tooLarge'], ['io', 'files.configIo']]) {
    expect(configErrorText(new ApiError(422, 'error.request', { file: code }), tr)).toBe(tr(key));
  }
  const errors = configErrorText(new ApiError(422, 'error.request', { frameRate: 'invalid', hotspotSecurity: 'unsupported' }), tr)!;
  expect(errors).toContain(tr('files.invalid', { key: 'frameRate' }));
  expect(errors).toContain(tr('files.unsupported')); expect(errors.split('\n')).toHaveLength(2);
});
it('manual checks bypass cache and keep in-flight channel changes without a second request', async () => {
  let finish!: (value: UpdateCheck) => void;
  const fetcher = vi.fn(() => new Promise<UpdateCheck>(resolve => { finish = resolve; }));
  const state = new UpdateState(fetcher), listener = vi.fn(), dispose = state.subscribe(listener);
  const pending = state.check(); state.setChannel('beta'); await state.check(); expect(fetcher).toHaveBeenCalledOnce();
  finish(result); await pending;
  expect(state.result?.latest).toBe('0.1.8-beta.1'); expect(state.result?.channel).toBe('beta');
  state.setChannel('stable'); expect(state.result?.latest).toBe('0.1.7'); expect(fetcher).toHaveBeenCalledOnce();
  const next = state.check(); finish(result); await next; expect(fetcher).toHaveBeenCalledTimes(2);
  expect(listener).toHaveBeenCalled(); dispose();
  const transport = vi.fn().mockResolvedValue(new Response(JSON.stringify(result))); vi.stubGlobal('fetch', transport);
  await api.updateCheck('stable');
  expect(transport.mock.calls[0][0]).toBe('/api/v0/update?channel=stable&fresh=1');
});
it('retains offline, timeout, rate-reset, HTTP status and invalid-response feedback', async () => {
  for (const failure of [{ kind: 'offline' }, { kind: 'timeout' }, { kind: 'rate-limited', resetMillis: 1793102400000 }, { kind: 'http', status: 502 }, { kind: 'invalid-response' }] as const) {
    const state = new UpdateState(async () => ({ channel: 'stable', failure })); await state.check();
    expect(state.failure).toEqual(failure); expect(state.checking).toBe(false);
    expect(updateFailureText(failure, t, 'en')).not.toContain('files.');
    expect(updateFailureText(failure, zh, 'zh-CN')).not.toContain('files.');
  }
  expect(updateFailureText({ kind: 'http', status: 502 }, t, 'en')).toContain('502');
  expect(updateFailureText({ kind: 'rate-limited', resetMillis: 1793102400000 }, t, 'en')).toContain('2026');
});
it('can reselect the last successful feed after a failed manual check', async () => {
  const fetcher = vi.fn().mockResolvedValueOnce(result).mockResolvedValueOnce({ channel: 'stable', failure: { kind: 'timeout' } });
  const state = new UpdateState(fetcher); await state.check(); await state.check();
  expect(state.failure?.kind).toBe('timeout'); expect(state.result).toBeNull();
  state.setChannel('beta'); expect(state.failure).toBeNull(); expect(state.result?.latest).toBe('0.1.8-beta.1');
  expect(fetcher).toHaveBeenCalledTimes(2);
});
it('matches Android bilingual summaries and formatted notes without rendering release HTML', () => {
  expect(localizedNotes(candidate.notes, false)).toEqual({ summary: 'Improved connection', blocks: [{ kind: 'heading', text: 'Changes' }, { kind: 'bullet', text: 'Faster Carlink' }] });
  expect(localizedNotes(candidate.notes, true)).toEqual({ summary: '连接改进', blocks: [{ kind: 'heading', text: '更新' }, { kind: 'bullet', text: '更快的 Carlink' }] });
  const markup = releaseMarkup({ ...result, notes: candidate.notes.replace('Faster', '<script>bad</script>') }, t, 'en');
  expect(markup).toContain('2026-10-27'); expect(markup).toContain('<details'); expect(markup).toContain('<h3>Changes</h3>');
  expect(markup).toContain('&lt;script&gt;'); expect(markup).not.toContain('<script>'); expect(markup).not.toContain('private provenance');
  expect(releaseMarkup({ ...result, outcome: 'up-to-date' }, t, 'en')).not.toContain('is available');
  expect(releaseMarkup({ ...result, latest: null, outcome: 'no-release' }, t, 'en')).toContain('No Linux release');
  expect(releaseMarkup({ ...result, url: 'javascript:alert(1)' }, t, 'en')).not.toContain('href=');
});
class Host extends EventTarget {
  innerHTML = ''; textContent = ''; disabled = false; hidden = false; isConnected = true; value = '';
  children = new Map<string, Host>();
  querySelector(selector: string): Host { if (!this.children.has(selector)) this.children.set(selector, new Host()); return this.children.get(selector)!; }
  toggleAttribute(name: string, value: boolean) { if (name === 'hidden') this.hidden = value; }
  click() { this.dispatchEvent(new Event('click')); }
}
it('keeps result and channel through navigation and language changes, with an About menu badge', async () => {
  const fetcher = vi.spyOn(api, 'updateCheck').mockResolvedValue(result);
  // The singleton's fetch callback is the API wrapper; stub fetch to exercise it as well.
  vi.stubGlobal('fetch', vi.fn().mockResolvedValue(new Response(JSON.stringify(result))));
  const options = { t, language: 'en', version: '0.1.6', platform: 'linux' as const, adminFeatures: ['update-check'], setLanguage: () => {}, error: () => 'error.request', reload: async () => {} };
  const first = new Host(); const dispose = mountFeaturePage(first as unknown as HTMLElement, 'settings/about', options);
  first.querySelector('#update-check').click(); await vi.waitFor(() => expect(updates.result?.latest).toBe('0.1.7')); dispose?.();
  updates.setChannel('beta');
  const second = new Host(); const close = mountFeaturePage(second as unknown as HTMLElement, 'settings/about', { ...options, t: zh, language: 'zh-CN' });
  expect(second.querySelector('#update-channel').value).toBe('beta');
  expect(second.querySelector('#update-result').innerHTML).toContain('连接改进');
  expect(updateMenuVersion('0.1.6', t)).toBe('0.1.6 · Update to 0.1.8-beta.1');
  expect(fetch).toHaveBeenCalledOnce(); expect(fetcher).not.toHaveBeenCalled(); close?.();
});
it.each(['en', 'zh-CN'] as const)('includes Android project, NOTICE, mixed licence, acknowledgements and trademark text (%s)', language => {
  const host = new Host(); const tr = language === 'en' ? t : zh;
  mountFeaturePage(host as unknown as HTMLElement, 'settings/about', { t: tr, language, version: 'test', platform: 'linux', adminFeatures: [], setLanguage: () => {}, error: () => '', reload: async () => {} });
  expect(host.innerHTML).toContain('https://openxplay.org'); expect(host.innerHTML).toContain('/blob/main/NOTICE');
  for (const key of ['files.licence', 'files.acknowledgementText', 'files.trademark']) expect(host.innerHTML).toContain(tr(key));
});
it.each(['en', 'zh-CN'] as const)('aligns About label/value and link rows and keeps language selection working (%s)', language => {
  const host = new Host(), setLanguage = vi.fn(), tr = language === 'en' ? t : zh;
  const dispose = mountFeaturePage(host as unknown as HTMLElement, 'settings/about', { t: tr, language, version: '<test>', platform: 'linux', adminFeatures: ['update-check'], setLanguage, error: () => '', reload: async () => {} });
  expect(host.innerHTML).toContain('<strong class="row-copy">OpenXPlay Relay</strong><span>&lt;test&gt;</span>');
  for (const key of ['files.project', 'files.source', 'files.licence']) expect(host.innerHTML).toContain(`<span class="row-copy">${tr(key)}</span>`);
  expect(host.innerHTML.match(/class="lucide lucide-external-link"/g)).toHaveLength(3);
  expect(host.innerHTML).toContain('<div class="inset-row"><label class="row-copy" for="language">');
  expect(host.innerHTML).toContain('<div class="input-row"><label class="row-copy" for="update-channel">');
  expect(host.querySelector('#language').value).toBe(language);
  host.querySelector('#language').value = 'auto';
  host.querySelector('#language').dispatchEvent(new Event('change'));
  expect(setLanguage).toHaveBeenCalledWith('auto');
  dispose?.();
});
