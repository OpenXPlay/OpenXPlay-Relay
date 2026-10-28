// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-20

import { Readable } from 'node:stream';
import type { IncomingMessage, ServerResponse } from 'node:http';
import type { Connect, ViteDevServer } from 'vite';
import { afterEach, expect, it, vi } from 'vitest';
import { mockApi, mockPublicName, mockSettings, previewStatus } from '../dev-server';
import { addressViews, homeButtonCount, publicNameUrl, settingsViews } from '../src/logic';
import { PublicNameDrafts, publicNameSubmission, publicNameGuidance, publicNameNameHint, importPublicNameFile, publicNameMessageLines, PUBLIC_NAME_MAX_BYTES, hasPublicName, publicNameConfigured, publicNameFileErrors, publicNameLine, publicNamePanel, TERMS_URL } from '../src/public-name';
import { addressesView } from '../src/views';
import { translate, type Language } from '../src/i18n';
import type { PublicNameStatus, Status } from '../src/api';
import { api } from '../src/api';
import type { PublicNameProviders } from '../src/schema';
import { createHash } from 'node:crypto';
afterEach(() => vi.unstubAllGlobals());

const en = (key: string, parameters?: Record<string, string | number>) => translate('en', key, parameters);
const zh = (key: string, parameters?: Record<string, string | number>) => translate('zh-CN', key, parameters);
const status = (publicName?: PublicNameStatus, receiverUrls: string[] = []): Status => ({ ...previewStatus({ host: 'openxplay-linux.local', receiverUrls } as unknown as Status, 'working'), receiverUrls, publicName });

it('uses the shared labels exactly, in both languages', () => {
  const labels: [string, string, string][] = [
    ['publicName.title', 'Trusted HTTPS', '可信 HTTPS'],
    ['publicName.summary', 'Open the head-unit page without a certificate warning, using a free domain name.', '用免费域名，让车机打开网页时不再出现证书警告。'],
    ['publicName.enabled', 'Use a free domain name', '使用免费域名'],
    ['publicName.provider', 'DNS provider', '域名服务商'],
    ['publicName.perProvider', 'Each provider keeps its own name and token. Switching shows what is saved for it.', '每个服务商的域名和 Token 分开保存，切换时会显示对应服务商已保存的内容。'],
    ['publicName.nameInvalid', 'Enter the full domain name. DuckDNS names end in .duckdns.org.', '请输入完整域名。DuckDNS 域名以 .duckdns.org 结尾。'],
    ['publicName.tokenInvalid', 'This token does not fit the chosen provider.', '这个 Token 与所选服务商不符。'],
    ['publicName.removeHint', 'Remove every provider’s name and token, and the certificate, from this relay? The address records at the providers stay.', '要从这台中继移除所有服务商的域名和 Token，以及证书吗？服务商那里的地址记录保持不变。'],
    ['publicName.name', 'Domain name', '域名'],
    ['publicName.nameHint', 'For example myrelay.duckdns.org', '例如 myrelay.duckdns.org'],
    ['publicName.token', 'Token', 'Token'],
    ['publicName.tokenHint', 'Copy it from your provider account. It stays on this relay.', '从服务商账号中复制，只保存在这台中继上。'],
    ['publicName.tokenSaved', 'Token saved', '已保存 Token'],
    ['publicName.terms', "I agree to the Let's Encrypt Subscriber Agreement", '我同意 Let\'s Encrypt 订户协议'],
    ['publicName.renew', 'Request certificate now', '立即申请证书'],
    ['publicName.remove', 'Remove', '移除'],
    ['publicName.status.off', 'Off', '未开启'],
    ['publicName.status.incomplete', 'Fill in the provider, domain name and token, and agree to the terms.', '请填写域名服务商、域名和 Token，并同意协议。'],
    ['publicName.status.waiting-network', 'Waiting for internet access', '等待联网'],
    ['publicName.status.updating-address', 'Updating the domain\'s address…', '正在更新域名地址…'],
    ['publicName.status.requesting-certificate', 'Requesting a certificate…', '正在申请证书…'],
    ['publicName.error.token-rejected', 'The provider rejected the token', '服务商拒绝了这个 Token'],
    ['publicName.error.name-not-found', "The provider doesn't know this domain name", '服务商找不到这个域名'],
    ['publicName.error.provider-rate-limited', 'The provider asked to wait', '服务商要求稍后再试'],
    ['publicName.error.provider-unreachable', "Can't reach the provider", '无法连接服务商'],
    ['publicName.error.dns-not-updated', "The provider's name servers didn't show the change in time", '服务商的域名服务器未及时生效'],
    ['publicName.error.acme-rate-limited', 'Let\'s Encrypt asked to wait', 'Let\'s Encrypt 要求稍后再试'],
    ['publicName.error.no-address', 'No head-unit network address yet', '还没有车机网络地址'],
  ];
  for (const [key, english, chinese] of labels) { expect(en(key), key).toBe(english); expect(zh(key), key).toBe(chinese); }
  expect(en('publicName.status.ready', { address: 'https://a.duckdns.org:8443/', date: 'Jan 19, 2027' })).toBe('Ready: https://a.duckdns.org:8443/ (certificate valid until Jan 19, 2027)');
  expect(zh('publicName.status.ready', { address: 'X', date: 'D' })).toBe('已就绪：X（证书有效期至 D）');
  expect(en('publicName.status.error', { problem: 'P', time: 'T' })).toBe('Problem: P. Next try at T.');
  expect(zh('publicName.status.error', { problem: 'P', time: 'T' })).toBe('出现问题：P。将于 T 重试。');
  expect(en('publicName.error.acme-rejected', { detail: 'D' })).toBe("Let's Encrypt rejected the request: D");
  expect(zh('publicName.error.acme-rejected', { detail: 'D' })).toBe('Let\'s Encrypt 拒绝了请求：D');
  for (const lang of ['en', 'zh-CN'] as Language[]) for (const key of ['publicName.removeHint', 'publicName.fallback', 'publicName.fileError', 'error.publicNameFile', ...['invalid', 'unknown-key', 'bad-version', 'too-large', 'not-json'].map(code => `publicName.file.${code}`)]) expect(translate(lang, key), `${lang}: ${key}`).not.toBe(key);
  // deSEC is gone from the page.
  expect(en('option.publicNameProvider.desec')).toBe('option.publicNameProvider.desec');
});

it('shows the status line for every state with the address, dates and the CA detail', () => {
  const now = 1_800_000_000;
  expect(publicNameLine(status(undefined), en, 'en')).toBe('');
  for (const state of ['off', 'incomplete', 'waiting-network', 'updating-address', 'requesting-certificate'] as const) expect(publicNameLine(status({ state }), en, 'en')).toBe(en(`publicName.status.${state}`));
  const ready = publicNameLine(status({ state: 'ready', name: 'myrelay.duckdns.org', trustedName: 'myrelay.duckdns.org', notAfter: now }, ['https://myrelay.duckdns.org:8443/#pair=1']), en, 'en');
  expect(ready).toContain('Ready: https://myrelay.duckdns.org:8443/#pair=1 (certificate valid until ');
  expect(ready).toContain('2027');
  expect(publicNameLine(status({ state: 'ready', name: 'new.duckdns.org' }), en, 'en')).toBe(en('publicName.state.ready'));
  const error = publicNameLine(status({ state: 'error', error: 'acme-rejected', errorDetail: 'CAA forbids', nextAttempt: now }), zh, 'zh-CN');
  expect(error).toMatch(/^出现问题：Let's Encrypt 拒绝了请求：CAA forbids。将于 .+ 重试。$/);
  expect(publicNameLine(status({ state: 'error', error: 'no-address', nextAttempt: now }), en, 'en')).toMatch(/^Problem: No head-unit network address yet\. Next try at .+\.$/);
  expect(publicNameFileErrors(status({ state: 'incomplete', fileErrors: { nmae: 'unknown-key' } }), en)).toEqual(['public-name.json, nmae: unknown key']);
});

it('places the section after the car screen settings, only when the relay offers it', () => {
  const linux = mockSettings(true);
  expect(hasPublicName(linux)).toBe(true);
  expect(hasPublicName(mockSettings())).toBe(false);
  const ids = settingsViews(linux, false).map(section => section.id);
  expect(ids.indexOf('publicName')).toBe(ids.indexOf('output') + 1);
  expect(ids.indexOf('publicName')).toBeLessThan(ids.indexOf('plugins'));
  const section = settingsViews(linux, false).find(s => s.id === 'publicName')!;
  expect(section.titleKey).toBe('publicName.title');
  expect(section.fields.map(f => f.key)).toEqual(['publicNameEnabled', 'publicNameProvider', 'publicName', 'publicNameToken', 'publicNameAgreeTerms']);
  expect(section.fields.find(f => f.key === 'publicNameProvider')!.options!.map(o => o.value)).toEqual(['duckdns', 'dynv6']);
  expect(section.fields.find(f => f.key === 'publicNameToken')!.type).toBe('secret');
  expect(section.fields.every(f => !f.visibleWhen)).toBe(true);
  // Its fields stay out of the other groups.
  expect(settingsViews(linux, true).filter(s => s.id !== 'publicName').flatMap(s => s.fields).some(f => f.key.startsWith('publicName'))).toBe(false);
  expect(settingsViews(mockSettings(), true).some(s => s.id === 'publicName')).toBe(false);
});

it('renders the panel with the agreement link, Request certificate now and Remove', () => {
  const settings = mockSettings(true);
  const off = publicNamePanel(status({ state: 'off' }), settings, false, '', en, 'en');
  expect(off).toContain('Off');
  expect(off).not.toContain('public-name-renew');
  expect(off).not.toContain('public-name-remove');
  expect(off).toContain(TERMS_URL);
  settings.values.publicNameEnabled = true; settings.values.publicNameTokenSet = true;
  expect(publicNameConfigured(settings.values)).toBe(true);
  const on = publicNamePanel(status({ state: 'error', error: 'token-rejected', nextAttempt: 1_800_000_000 }), settings, true, 'error.request', en, 'en');
  expect(on).toContain(`href="${TERMS_URL}"`);
  expect(on).toContain('id="public-name-renew" type="button" disabled>Request certificate now');
  expect(on).toContain('id="public-name-remove"');
  expect(on).toContain('class="error" role="status"');
  expect(on).toContain(en('error.request'));
  // Escaped: a CA detail never becomes markup.
  expect(publicNamePanel(status({ state: 'error', error: 'acme-rejected', errorDetail: '<b>x</b>' }), settings, false, '', en, 'en')).toContain('&lt;b&gt;x&lt;/b&gt;');
  for (const state of ['off', 'incomplete', 'waiting-network', 'updating-address', 'requesting-certificate', 'ready'] as const) {
    expect(publicNamePanel(status({ state }), settings, false, '', en, 'en')).not.toContain('public-name-renew');
  }
});

it('keeps independent provider drafts, saved flags and newer edits across a completed save', () => {
  const values = mockSettings(true).values;
  values.publicNameProviders = { duckdns: { name: 'duck.duckdns.org', tokenSet: true }, dynv6: { name: 'dyn.dynv6.net', tokenSet: false } };
  const drafts = new PublicNameDrafts();
  drafts.select('duckdns', values);
  expect(values).toMatchObject({ publicName: 'duck.duckdns.org', publicNameToken: '', publicNameTokenSet: true });
  values.publicName = 'typed.duckdns.org'; values.publicNameToken = 'typed-duck'; drafts.edit('duckdns', values);
  const submitted = publicNameSubmission(values);
  drafts.select('dynv6', values);
  expect(values).toMatchObject({ publicName: 'dyn.dynv6.net', publicNameToken: '', publicNameTokenSet: false });
  values.publicName = 'typed.dynv6.net'; values.publicNameToken = 'typed-dyn'; drafts.edit('dynv6', values);
  expect(publicNameSubmission(values)).toMatchObject({ publicNameProvider: 'dynv6', publicName: 'typed.dynv6.net', publicNameToken: 'typed-dyn' });
  drafts.select('duckdns', values);
  expect(values).toMatchObject({ publicName: 'typed.duckdns.org', publicNameToken: 'typed-duck', publicNameTokenSet: true });
  values.publicName = 'newer.duckdns.org'; drafts.edit('duckdns', values);
  drafts.saved(submitted, values);
  expect(values).toMatchObject({ publicName: 'newer.duckdns.org', publicNameToken: '', publicNameTokenSet: true });
  drafts.select('dynv6', values);
  expect(values).toMatchObject({ publicName: 'typed.dynv6.net', publicNameToken: 'typed-dyn', publicNameTokenSet: false });
  const dynSubmitted = publicNameSubmission(values);
  values.publicNameToken = 'newer-token'; drafts.edit('dynv6', values);
  drafts.saved(dynSubmitted, values);
  expect(values.publicNameToken).toBe('newer-token');
  drafts.select('duckdns', values);
  drafts.saved(publicNameSubmission(values), values);
  expect(values).toMatchObject({ publicName: 'newer.duckdns.org', publicNameToken: '', publicNameTokenSet: true });
  // Untouched providers follow saved changes, while edited providers retain their drafts.
  (values.publicNameProviders as PublicNameProviders).duckdns.name = 'updated.duckdns.org';
  drafts.select('duckdns', values);
  expect(values.publicName).toBe('updated.duckdns.org');
  drafts.select('dynv6', values);
  expect(values.publicNameToken).toBe('newer-token');
  drafts.imported('duckdns', values);
  expect(values.publicName).toBe('updated.duckdns.org');
  drafts.select('dynv6', values);
  expect(values.publicNameToken).toBe('newer-token');
});

it('shows the provider guidance, name examples and safe external links in both languages', () => {
  for (const language of ['en', 'zh-CN'] as const) for (const provider of ['duckdns', 'dynv6']) {
    const t = (key: string) => translate(language, key);
    const values = { publicNameProvider: provider };
    const html = publicNameGuidance(values, t, language);
    expect(html).toContain(t('publicName.perProvider'));
    expect(html).toContain(provider === 'duckdns' ? 'href="https://www.duckdns.org/"' : 'href="https://dynv6.com/"');
    expect(html).toContain(`href="https://openxplay.org/${language === 'en' ? 'en/' : ''}trusted-https"`);
    expect(html.match(/target="_blank" rel="noopener"/g)).toHaveLength(2);
    expect(html).toContain(t(`publicName.open.${provider}`));
    expect(html).toContain(t('publicName.guide'));
    expect(html).toContain(provider === 'duckdns' ? 'GitHub' : 'HTTP');
    expect(t(publicNameNameHint(values))).toContain(provider === 'duckdns' ? 'myrelay.duckdns.org' : 'myrelay.dynv6.net');
  }
  expect(publicNameGuidance({ publicNameProvider: '' }, en, 'en')).toContain(en('publicName.perProvider'));
});

it('reads the selected file in the browser, sends content once and reports success and every file error', async () => {
  const content = '{"version":1,"token":"private"}';
  const imported = { ok: true as const, provider: 'dynv6', name: 'myrelay.dynv6.net' };
  const fetcher = vi.fn().mockResolvedValue(new Response(JSON.stringify(imported)));
  vi.stubGlobal('fetch', fetcher);
  const text = vi.fn().mockResolvedValue(content);
  expect(await importPublicNameFile({ size: PUBLIC_NAME_MAX_BYTES, text })).toEqual(imported);
  expect(text).toHaveBeenCalledOnce();
  expect(fetcher).toHaveBeenCalledWith('/api/v0/public-name/import', expect.objectContaining({ method: 'POST', credentials: 'same-origin', headers: { 'Content-Type': 'application/json', 'X-OpenXPlay-Admin': '1' }, body: JSON.stringify({ content }) }));
  for (const [t, language] of [[en, 'en'], [zh, 'zh-CN']] as const) {
    const lines = publicNameMessageLines(imported, t);
    expect(lines).toEqual([language === 'en' ? 'Imported: dynv6, myrelay.dynv6.net' : '已导入：dynv6，myrelay.dynv6.net']);
    const html = publicNamePanel(status(), mockSettings(true), false, imported, t, language);
    expect(html).toContain(lines[0]);
    expect(html).toContain('accept=".json"');
    expect(html).toContain(t('publicName.import'));
    const errors = Object.fromEntries(['missing', 'invalid', 'unknown-key', 'bad-version', 'too-large', 'not-json'].map(code => [code, code]));
    const messages = publicNameMessageLines({ errors }, t);
    expect(messages).toHaveLength(6);
    for (const code of Object.keys(errors)) expect(messages).toContain(t('publicName.fileError', { key: code, problem: t(`publicName.file.${code}`) }));
  }
  fetcher.mockClear(); text.mockClear();
  await expect(importPublicNameFile({ size: PUBLIC_NAME_MAX_BYTES + 1, text })).rejects.toMatchObject({ errors: { file: 'too-large' } });
  expect(text).not.toHaveBeenCalled(); expect(fetcher).not.toHaveBeenCalled();
  fetcher.mockResolvedValueOnce(new Response(JSON.stringify({ errors: { token: 'invalid', nmae: 'unknown-key' } }), { status: 422 }));
  await expect(api.publicNameImport(content)).rejects.toMatchObject({ errors: { token: 'invalid', nmae: 'unknown-key' } });
  fetcher.mockRejectedValue(new Error('lost response')); fetcher.mockClear();
  await expect(api.publicNameImport(content)).rejects.toBeInstanceOf(Error);
  expect(fetcher).toHaveBeenCalledOnce();
});

it('renders the ordered Linux admin URLs with copy buttons and preserves the Android page', () => {
  const base = status(undefined, ['https://192.168.1.78:8443/']);
  const adminUrls = ['https://myrelay.duckdns.org:6975/', 'https://10.77.0.1:6975/', 'https://192.168.1.78:6975/', 'https://openxplay-linux.local:6975/'];
  for (const t of [en, zh]) {
    const html = addressesView({ ...base, platform: 'linux', adminUrls }, false, t);
    expect(html.match(/class="address-row admin-address"/g)).toHaveLength(4);
    expect(html).toContain(t('home.adminPage'));
    for (const url of adminUrls) { expect(html).toContain(`data-copy="${url}"`); expect(html).toContain(`href="${url}" target="_blank" rel="noopener"`); }
    expect(adminUrls.map(url => html.indexOf(`href="${url}"`))).toEqual([...adminUrls.map(url => html.indexOf(`href="${url}"`))].sort((a, b) => a - b));
    expect(homeButtonCount({ ...base, platform: 'linux', adminUrls })).toBe(homeButtonCount({ ...base, platform: 'linux' }) + adminUrls.length - 1);
    const android = { ...base, platform: 'android' as const, publicName: undefined, adminUrls: undefined };
    expect(publicNamePanel(android, mockSettings(), false, '', t, 'en')).toBe('');
    expect(hasPublicName(mockSettings())).toBe(false);
    const original = addressesView(android, false, t);
    expect(original).toContain(t('ui.admin'));
    expect(original).not.toContain(t('home.adminPage'));
    expect(addressesView({ ...android, adminUrls }, false, t)).toBe(original);
    // Golden markup from the Android-shaped page before the Linux address-list extension.
    expect(createHash('sha256').update(original).digest('hex')).toBe(t === en ? '2b67903e1c45b0052b4f831fcec72717c65d6dcb2b25894920aef073437dc552' : 'def560b45bb81e21e6af4caadeb1c850d265a885621263c4863d6cc358fbe3db');
  }
});

it('lists the name first on Home while ready and keeps the IP as fallback, within the button budget', () => {
  const ready: PublicNameStatus = { state: 'ready', name: 'myrelay.duckdns.org', address: '192.168.1.78', notAfter: 1_800_000_000, trustedName: 'myrelay.duckdns.org' };
  const plain = status(undefined, ['https://192.168.1.78:8443/']);
  const named = status(ready, ['https://myrelay.duckdns.org:8443/', 'https://192.168.1.78:8443/']);
  expect(publicNameUrl(plain)).toBeNull();
  expect(publicNameUrl(named)).toBe('https://myrelay.duckdns.org:8443/');
  // A failing renewal keeps a name that still works; an untrusted one is not shown.
  expect(publicNameUrl(status({ ...ready, state: 'error', error: 'dns-not-updated' }, named.receiverUrls))).toBe('https://myrelay.duckdns.org:8443/');
  expect(publicNameUrl(status({ ...ready, trustedName: undefined }, named.receiverUrls))).toBeNull();
  expect(addressViews(plain, false)[0]).toMatchObject({ receiver: 'https://192.168.1.78:8443/', fallback: null });
  expect(addressViews(named, false)[0]).toMatchObject({ receiver: 'https://myrelay.duckdns.org:8443/', fallback: 'https://192.168.1.78:8443/' });
  const html = addressesView(named, false, en);
  expect(html).toContain('href="https://myrelay.duckdns.org:8443/"');
  expect(html).toContain('Without the name: https://192.168.1.78:8443/');
  expect(homeButtonCount(named, true)).toBe(homeButtonCount(plain, true));
});

function relay() {
  let handler: Connect.NextHandleFunction;
  const plugin = mockApi(true);
  (plugin.configureServer as (server: ViteDevServer) => void)({ middlewares: { use: (callback: Connect.NextHandleFunction) => { handler = callback; } } } as unknown as ViteDevServer);
  let cookie = '';
  return async (path: string, method = 'GET', body: unknown = {}) => {
    const req = Readable.from([JSON.stringify(body)]) as IncomingMessage;
    req.url = `/api/v0${path}`; req.method = method;
    req.headers = { cookie, 'content-type': 'application/json', 'x-openxplay-admin': '1' };
    Object.defineProperty(req, 'socket', { value: { remoteAddress: '127.0.0.1' } });
    const headers: Record<string, string> = {};
    return new Promise<{ status: number; data: any }>((resolve, reject) => {
      const res = { statusCode: 200, setHeader: (key: string, value: string) => { headers[key] = value; }, end: (raw: string) => {
        if (headers['Set-Cookie']) cookie = headers['Set-Cookie'].split(';')[0];
        resolve({ status: res.statusCode, data: JSON.parse(raw) });
      } };
      handler(req, res as unknown as ServerResponse, error => reject(error ?? new Error('unhandled route')));
    });
  };
}
it('the Linux mock saves field by field, keeps the token secret and removes the configuration', async () => {
  const call = relay();
  await call('/login', 'POST', { password: 'openxplay' });
  expect((await call('/status')).data.publicName).toEqual({ state: 'off' });
  const token = 'a1b2c3d4-0000-0000-0000-000000000000';
  for (const [key, value] of [['publicNameEnabled', true], ['publicNameProvider', 'duckdns'], ['publicName', 'MyRelay.duckdns.org'], ['publicNameToken', token]] as const) {
    const saved = await call('/settings', 'PUT', { [key]: value });
    expect(saved.data, key).toEqual({ ok: true, applied: 'now' });
  }
  expect((await call('/status')).data.publicName.state).toBe('incomplete');
  expect((await call('/settings', 'PUT', { publicNameProvider: 'desec' })).status).toBe(422);
  expect((await call('/settings', 'PUT', { publicNameAgreeTerms: true })).status).toBe(200);
  const settings = (await call('/settings')).data;
  expect(JSON.stringify(settings)).not.toContain(token);
  expect(settings.values).toMatchObject({ publicName: 'myrelay.duckdns.org', publicNameToken: '', publicNameTokenSet: true });
  // An empty token keeps the saved one.
  await call('/settings', 'PUT', { publicNameToken: '' });
  expect((await call('/settings')).data.values.publicNameTokenSet).toBe(true);
  const ready = (await call('/status')).data;
  expect(ready.publicName).toMatchObject({ state: 'ready', name: 'myrelay.duckdns.org' });
  expect(ready.receiverUrls[0]).toBe('https://myrelay.duckdns.org:8443/');
  expect((await call('/actions/public-name-renew', 'POST')).status).toBe(200);
  expect((await call('/actions/public-name-remove', 'POST')).status).toBe(200);
  expect((await call('/status')).data.publicName).toEqual({ state: 'off' });
  expect((await call('/settings')).data.values).toMatchObject({ publicNameEnabled: false, publicNameTokenSet: false, publicName: '' });
  expect(mockPublicName({ publicNameEnabled: true, publicNameProvider: 'dynv6', publicName: 'fail.dynv6.net', publicNameTokenSet: true, publicNameAgreeTerms: true }, 0)).toMatchObject({ state: 'error', error: 'acme-rejected' });
});

it('the Linux mock retains both providers and activates an imported setup without exposing tokens', async () => {
  const call = relay();
  await call('/login', 'POST', { password: 'openxplay' });
  const token = 'a1b2c3d4-0000-0000-0000-000000000000';
  await call('/settings', 'PUT', { publicNameProvider: 'duckdns', publicName: 'a.duckdns.org', publicNameToken: token });
  await call('/settings', 'PUT', { publicNameProvider: 'dynv6' });
  expect((await call('/settings')).data.values).toMatchObject({ publicName: '', publicNameTokenSet: false });
  const content = JSON.stringify({ version: 1, provider: 'dynv6', name: 'B.dynv6.net', token: 'dyn-secret', agreeLetsEncryptTerms: true });
  expect((await call('/public-name/import', 'POST', { content })).data).toEqual({ ok: true, provider: 'dynv6', name: 'b.dynv6.net' });
  const values = (await call('/settings')).data.values;
  expect(values).toMatchObject({ publicNameProvider: 'dynv6', publicName: 'b.dynv6.net', publicNameToken: '', publicNameTokenSet: true, publicNameProviders: { duckdns: { name: 'a.duckdns.org', tokenSet: true }, dynv6: { name: 'b.dynv6.net', tokenSet: true } } });
  expect(JSON.stringify(values)).not.toContain('dyn-secret');
  expect(JSON.stringify(values)).not.toContain(token);
  await call('/settings', 'PUT', { publicNameProvider: 'duckdns', publicNameToken: '' });
  expect((await call('/settings')).data.values).toMatchObject({ publicName: 'a.duckdns.org', publicNameTokenSet: true });
  expect((await call('/public-name/import', 'POST', { content: 'not json' })).data).toEqual({ errors: { file: 'not-json' } });
  expect((await call('/public-name/import', 'POST', { content: content.replace('"version":1', '"version":2') })).data).toEqual({ errors: { version: 'bad-version' } });
  expect((await call('/public-name/import', 'POST', { content: JSON.stringify({ ...JSON.parse(content), typo: 1 }) })).data).toEqual({ errors: { typo: 'unknown-key' } });
  expect((await call('/public-name/import', 'POST', { content: ' '.repeat(16385) })).data).toEqual({ errors: { file: 'too-large' } });
  await call('/actions/public-name-remove', 'POST');
  expect((await call('/settings')).data.values.publicNameProviders).toEqual({ duckdns: { name: '', tokenSet: false }, dynv6: { name: '', tokenSet: false } });
});

it('prepares provider configuration while off and reports private-data availability without secrets', async () => {
  const call = relay(); await call('/login', 'POST', { password: 'openxplay' });
  expect((await call('/settings')).data.values.privacyConfigAvailable).toBe(false);
  await call('/settings', 'PUT', { publicNameProvider: 'dynv6', publicName: 'car.example.org', publicNameToken: 'prepared-token' });
  const saved = (await call('/settings')).data;
  expect(saved.values).toMatchObject({ publicNameEnabled: false, privacyConfigAvailable: true, publicName: 'car.example.org', publicNameTokenSet: true });
  expect(JSON.stringify(saved)).not.toContain('prepared-token');
  expect((await call('/status')).data.publicName).toEqual({ state: 'off' });
  expect(saved.schema.find((s: { id: string }) => s.id === 'publicName').fields.every((f: { visibleWhen?: unknown }) => !f.visibleWhen)).toBe(true);
});
