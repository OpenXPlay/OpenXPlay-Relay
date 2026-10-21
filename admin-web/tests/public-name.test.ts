// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-20

import { Readable } from 'node:stream';
import type { IncomingMessage, ServerResponse } from 'node:http';
import type { Connect, ViteDevServer } from 'vite';
import { expect, it } from 'vitest';
import { mockApi, mockPublicName, mockSettings, previewStatus } from '../dev-server';
import { addressViews, homeButtonCount, publicNameUrl, settingsViews } from '../src/logic';
import { hasPublicName, publicNameConfigured, publicNameFileErrors, publicNameLine, publicNamePanel, TERMS_URL } from '../src/public-name';
import { addressesView } from '../src/views';
import { translate, type Language } from '../src/i18n';
import type { PublicNameStatus, Status } from '../src/api';

const en = (key: string, parameters?: Record<string, string | number>) => translate('en', key, parameters);
const zh = (key: string, parameters?: Record<string, string | number>) => translate('zh-CN', key, parameters);
const status = (publicName?: PublicNameStatus, receiverUrls: string[] = []): Status => ({ ...previewStatus({ host: 'openxplay-linux.local', receiverUrls } as unknown as Status, 'working'), receiverUrls, publicName });

it('uses the shared labels exactly, in both languages', () => {
  const labels: [string, string, string][] = [
    ['publicName.title', 'Trusted HTTPS', '可信 HTTPS'],
    ['publicName.summary', 'Open the head-unit page without a certificate warning, using a free domain name.', '用免费域名，让车机打开网页时不再出现证书警告。'],
    ['publicName.enabled', 'Use a free domain name', '使用免费域名'],
    ['publicName.provider', 'DNS provider', '域名服务商'],
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
  expect(off).not.toContain(TERMS_URL);
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
