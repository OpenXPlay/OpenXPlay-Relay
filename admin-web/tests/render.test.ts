// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-27

// Optional real-DOM gate: ADMIN_BROWSER_URL points at an isolated Chrome CDP endpoint,
// ADMIN_PREVIEW_URL at `node dev.mjs --states`. Uses Node's built-in WebSocket, no dependency.
import { readFileSync, writeFileSync } from 'node:fs';
import { afterAll, beforeAll, describe, expect, it } from 'vitest';
import { linuxSettings, linuxStatusFixtures, mockSettings } from '../dev-server';
import { developerPages, pageKeys, pages } from '../src/settings-tree';
import { setupSteps } from '../src/setup-guide';
import { mockPathInput } from '../dev-fixtures';
import type { Status } from '../src/api';

const browser = process.env.ADMIN_BROWSER_URL, preview = process.env.ADMIN_PREVIEW_URL ?? 'http://127.0.0.1:5173/';
class Cdp {
  socket!: WebSocket;
  next = 0;
  pending = new Map<number, { resolve: (value: any) => void; reject: (error: Error) => void }>();
  async open(url: string) {
    this.socket = new WebSocket(url);
    await new Promise<void>((resolve, reject) => { this.socket.onopen = () => resolve(); this.socket.onerror = () => reject(new Error('Chrome CDP unavailable')); });
    this.socket.onmessage = event => { const message = JSON.parse(String(event.data)); const task = this.pending.get(message.id); if (!task) return; this.pending.delete(message.id); if (message.error) task.reject(new Error(JSON.stringify(message.error))); else task.resolve(message.result); };
  }
  call(method: string, params: Record<string, unknown> = {}): Promise<any> {
    const id = ++this.next;
    return new Promise((resolve, reject) => { this.pending.set(id, { resolve, reject }); this.socket.send(JSON.stringify({ id, method, params })); });
  }
  async js(expression: string) { const result = await this.call('Runtime.evaluate', { expression, returnByValue: true, awaitPromise: true }).catch(error => { throw new Error(`${error.message} in ${expression}`); }); if (result.exceptionDetails) throw new Error(JSON.stringify(result.exceptionDetails)); return result.result.value; }
}
function renderSource(status: Status, settings: unknown, language: string, token = "fixture") {
  return `window.fixtureToken=${JSON.stringify(token)}; localStorage.setItem('openxplay.language', ${JSON.stringify(language)}); window.renderErrors = []; window.adminWrites = []; window.adminCalls = []; window.addEventListener('error', e => window.renderErrors.push(e.message)); window.addEventListener('unhandledrejection', e => window.renderErrors.push(String(e.reason))); const fixtureStatus = ${JSON.stringify(status)}, fixtureSettings = ${JSON.stringify(settings)}; window.fetch = async (url, init = {}) => { const path = String(url).split('?')[0]; if(init.method && init.method !== 'GET') window.adminCalls.push(path); let data = {}; let code = 200; if (path.endsWith('/session')) data = {state:'ready',defaultPassword:false}; else if (path.endsWith('/settings')) { if (init.method === 'PUT') { window.adminWrites.push(JSON.parse(init.body)); Object.assign(fixtureSettings.values, JSON.parse(init.body)); data={ok:true,applied:'now'}; } else data=fixtureSettings; } else if (path.endsWith('/status')) data=fixtureStatus; else if (path.endsWith('/identity')) data=fixtureStatus.identity; else if (path.endsWith('/phones')) data=[]; else if (path.endsWith('/logs/receipt')) code=404; else if (path.endsWith('/carlink-trace')) data={enabled:false,running:false,status:null,files:[]}; else if (path.includes('/carlink-trace/tail')) data={lines:[]}; return new Response(JSON.stringify(data), {status:code,headers:{'Content-Type':'application/json'}}); };`;
}
const routes = ['home', 'settings', ...setupSteps.map(step => 'setup/' + step), ...new Set([...pages.map(([page]) => 'settings/' + page), ...Object.keys(pageKeys).map(page => 'settings/' + page), ...developerPages.map(page => 'settings/developer/' + page), 'settings/access/admin'])];
const carlinkSettings = JSON.parse(readFileSync(new URL('./fixtures/linux/settings-carlink.json', import.meta.url), 'utf8'));
const usbSettings = JSON.parse(readFileSync(new URL('./fixtures/linux/settings-usb.json', import.meta.url), 'utf8'));
const cases = Object.entries(linuxStatusFixtures).map(([state, status]) => ({ state, platform: 'linux', status, settings: status.pathInput?.setup?.network === 'USB' ? usbSettings : status.output === 'web' ? linuxSettings : carlinkSettings }));
for (const state of ['idle', 'ready', 'pairing', 'carlink-scanning', 'carlink-pin', 'failure', 'live']) {
  const status = structuredClone(linuxStatusFixtures.ready) as Status;
  status.platform = 'android'; delete status.adminFeatures; delete status.hostProfile; delete status.publicName;
  status.pathInput = mockPathInput(status, state);
  cases.push({ state, platform: 'android', status, settings: mockSettings() });
}
describe.skipIf(!browser)('phone route rendering with production responses', () => {
  const cdp = new Cdp(); let target = '', script = '';
  async function freshPage() {
    cdp.socket?.close();
    if (target) await fetch(`${browser}/json/close/${target}`);
    const page = await (await fetch(`${browser}/json/new?about:blank`, { method: 'PUT' })).json();
    target = page.id; script = ''; await cdp.open(page.webSocketDebuggerUrl);
    await cdp.call('Page.enable');
    await cdp.call('Emulation.setDeviceMetricsOverride', { width: 390, height: 844, deviceScaleFactor: 1, mobile: true });
    await cdp.call('Emulation.setEmulatedMedia', { features: [{ name: 'prefers-reduced-motion', value: 'reduce' }] });
  }
  beforeAll(freshPage);
  afterAll(async () => { cdp.socket?.close(); if (target) await fetch(`${browser}/json/close/${target}`); });
  for (const language of ['en', 'zh-CN']) it.each(cases)(`renders every route without unresolved keys or invalid values (${language}, $platform/$state)`, async ({ status, settings, state }) => {
    if (script) await cdp.call('Page.removeScriptToEvaluateOnNewDocument', { identifier: script });
    const token = `${status.platform}-${state}-${language}`;
    const source = renderSource(status, settings, language, token);
    script = (await cdp.call('Page.addScriptToEvaluateOnNewDocument', { source })).identifier;
    await cdp.call('Page.navigate', { url: preview + '?render=' + Date.now() + '#' + routes[0] });
    await cdp.js(`new Promise(resolve => { const timer = setInterval(() => { if (window.fixtureToken === ${JSON.stringify(token)} && document.querySelector('#stage-headline, .stage-text h1, #stage-primary')) { clearInterval(timer); resolve(true); } }, 10); setTimeout(() => { clearInterval(timer); resolve(false); }, 5000); })`);
    for (const theme of ['light', 'dark']) {
    await cdp.call('Emulation.setEmulatedMedia', { features: [{ name: 'prefers-color-scheme', value: theme }, { name: 'prefers-reduced-motion', value: 'reduce' }] });
    for (const route of routes) {
      await cdp.js(`document.querySelectorAll('dialog').forEach(d => d.remove()); history.replaceState(null,''); location.hash = ${JSON.stringify(route)}; new Promise(r => setTimeout(r, 20))`);
      const result = await cdp.js(`({ text: document.body.innerText, errors: window.renderErrors, width: document.documentElement.scrollWidth, viewport: innerWidth, buttons: [...document.querySelectorAll('button, a')].filter(e => e.getClientRects().length).map(e=>e.getAttribute('aria-label')||e.textContent) })`);
      expect(result.errors, route).toEqual([]);
      expect(result.text, route).not.toMatch(/\bundefined\b|\bnull\b|\[object Object\]|\b(?:publicName|field|option|section|menu|stage|roles|files|picture)\.[a-zA-Z][\w./-]+|\b(?:path|stage|logs|error_explain)_[a-z_]+/);
      expect(result.width, route).toBeLessThanOrEqual(result.viewport);
      if (status.platform === 'android' && route === 'settings/about') expect(result.text).not.toMatch(/Flasher|Linux/);
      if (process.env.ADMIN_SCREENSHOTS && status.platform === 'linux' && ['ready', 'live-carlink'].includes(state)) {
        const shot = await cdp.call('Page.captureScreenshot', { format: 'png' });
        writeFileSync(`${process.env.ADMIN_SCREENSHOTS}/${state}-${language}-${theme}-${route.replaceAll('/', '-')}.png`, Buffer.from(shot.data, 'base64'));
      }
    }
    }
  }, 30000);
  it('confirms Linux USB with cable advice and hides the Bluetooth picker after saving', async () => {
    await showFixture(linuxStatusFixtures.ready, 'settings/connection', linuxSettings);
    await cdp.js(`document.querySelector('#field-transport').click(); new Promise(r=>setTimeout(r,50))`);
    expect(await cdp.js(`document.querySelector('dialog').innerText`)).toContain('USB-A');
    await cdp.js(`document.querySelector('dialog [data-option="1"]').click(); new Promise(r=>setTimeout(r,100))`);
    const advice = await cdp.js(`document.querySelector('dialog:last-of-type').innerText`);
    expect(advice).toContain('USB-A to Lightning');
    expect(advice).toContain('Charge-only cables do not work');
    expect(advice).not.toContain('battery');
    expect(await cdp.js('window.adminWrites')).toEqual([]);
    await cdp.js(`document.querySelector('dialog:last-of-type [data-confirm]').click(); new Promise(r=>setTimeout(r,600))`);
    expect(await cdp.js('window.adminWrites.at(-1)')).toEqual({ transport: 'usb' });
    expect(await cdp.js(`document.querySelector('#field-iphone').closest('.field').hidden`)).toBe(true);
    expect(await cdp.js('window.renderErrors')).toEqual([]);
  }, 10000);
  it('edits saved sizes atomically, tries without saving, and uses phone controls', async () => {
    await showFixture(linuxStatusFixtures['live-carlink'], 'settings/picture');
    const add = async (width: number, height: number) => cdp.js(`{ const form = document.querySelector('.picture-size-form'); form.elements.width.value='${width}'; form.elements.height.value='${height}'; form.dispatchEvent(new Event('input', {bubbles:true})); form.requestSubmit(); } new Promise(r=>setTimeout(r,20))`);
    await add(1440, 900);
    expect(await cdp.js('window.adminWrites.at(-1)')).toEqual({ carlinkPictureSizes: [{ width: 1440, height: 900 }], carlinkPictureTry: { width: 1440, height: 900 } });
    await cdp.js(`document.querySelector('[data-default]').click(); new Promise(r=>setTimeout(r,20))`);
    expect(await cdp.js('window.adminWrites.at(-1)')).toEqual({ carlinkPictureDefault: { width: 1440, height: 900 } });
    await cdp.js(`document.querySelector('[data-size="0"]').click(); new Promise(r=>setTimeout(r,20))`);
    expect(await cdp.js('window.adminWrites.at(-1)')).toEqual({ carlinkPictureTry: null });
    await cdp.js(`document.querySelector('[data-delete]').click(); new Promise(r=>setTimeout(r,20))`);
    expect(await cdp.js('window.adminWrites.at(-1)')).toEqual({ carlinkPictureSizes: [], carlinkPictureDefault: null });
    for (let i = 0; i < 5; i++) await add(1300 + i * 2, 720);
    expect(await cdp.js(`document.querySelector('.picture-size-form button').disabled`)).toBe(true);
    for (const route of ['settings/logs', 'settings/config-files', 'settings/connection/network']) {
      await cdp.js(`location.hash='${route}'; new Promise(r=>setTimeout(r,20))`);
      if (route.endsWith('/logs')) {
        expect(await cdp.js(`document.querySelector('#logs-upload').classList.contains('primary')`)).toBe(true);
        expect(await cdp.js(`document.querySelector('#logs-save').getBoundingClientRect().width`)).toBeGreaterThan(300);
        await cdp.js(`document.querySelector('#logs-privacy').click(); new Promise(r=>setTimeout(r,20))`);
        expect(await cdp.js(`document.querySelector('dialog').innerText`)).toContain('Cloudflare');
        await cdp.js(`document.querySelector('dialog [data-close]').click(); new Promise(r=>setTimeout(r,20))`);
      } else if (route.endsWith('/config-files')) {
        expect(await cdp.js(`document.querySelector('#config-file').hidden`)).toBe(true);
        expect(await cdp.js(`document.querySelector('#choose-config').getBoundingClientRect().width`)).toBeGreaterThan(300);
      } else {
        await cdp.js(`document.querySelector('#field-wifiCountry').click(); new Promise(r=>setTimeout(r,20))`);
        expect(await cdp.js(`document.querySelectorAll('dialog [data-option]').length`)).toBeGreaterThan(200);
        expect(await cdp.js(`document.querySelector('dialog').innerText`)).toContain('China');
        await cdp.js(`document.querySelector('dialog [data-close]').click();new Promise(r=>setTimeout(r,100))`);
      }
    }
  }, 30000);

  it('keeps text within a 360 px phone at 130% and closes nested save sheets', async () => {
    await showFixture(linuxStatusFixtures['live-browser'], 'settings/picture', linuxSettings);
    await cdp.js(`document.documentElement.style.fontSize='130%'`);
    await cdp.call('Emulation.setDeviceMetricsOverride', { width: 360, height: 800, deviceScaleFactor: 1, mobile: true });
    for (const route of ['settings', 'settings/output', 'settings/car', 'settings/picture', 'settings/logs', 'settings/config-files', 'settings/about', 'settings/connection/network']) {
      await cdp.js(`location.hash='${route}'; new Promise(r=>setTimeout(r,30))`);
      expect(await cdp.js('document.documentElement.scrollWidth'), route).toBeLessThanOrEqual(360);
    }
    await cdp.js(`location.hash='settings/picture'; new Promise(r=>setTimeout(r,30))`);
    await cdp.js(`document.querySelector('#field-resolution').click(); new Promise(r=>setTimeout(r,30))`);
    await cdp.js(`document.querySelector('dialog [data-option="3"]').click(); new Promise(r=>setTimeout(r,30))`);
    expect(await cdp.js('document.querySelectorAll("dialog[open]").length')).toBe(2);
    await cdp.js(`document.querySelector('dialog [data-confirm]').click(); new Promise(r=>setTimeout(r,100))`);
    expect(await cdp.js('document.querySelectorAll("dialog[open]").length')).toBe(0);
    expect(await cdp.js('history.state?.oxpSheet ?? null')).toBeNull();
    expect(await cdp.js('window.renderErrors')).toEqual([]);
  }, 10000);

  async function screenshot(name: string) {
    if (!process.env.ADMIN_W14_SCREENSHOTS) return;
    const shot = await cdp.call('Page.captureScreenshot', { format: 'png' });
    writeFileSync(`${process.env.ADMIN_W14_SCREENSHOTS}/${name}.png`, Buffer.from(shot.data, 'base64'));
  }
  async function showFixture(status: Status, hash = 'home', fixtureSettings = carlinkSettings) {
    await freshPage();
    const token = 'interaction-' + Math.random();
    script = (await cdp.call('Page.addScriptToEvaluateOnNewDocument', { source: renderSource(status, fixtureSettings, 'en', token) })).identifier;
    await cdp.call('Page.navigate', { url: preview + '?w14=' + Date.now() + '#' + hash });
    await cdp.js(`new Promise(resolve=>{const timer=setInterval(()=>{if(window.fixtureToken===${JSON.stringify(token)} && document.querySelector('#stage-headline,.settings-page')){clearInterval(timer);resolve(true)}},10);setTimeout(()=>{clearInterval(timer);resolve(false)},5000)})`);
  }
  it('keeps address selection across drawer updates and exposes complete step details', async () => {
    const status = structuredClone(linuxStatusFixtures.ready);
    status.hostAddresses!.push({ interface: 'eth0', ipv4: '192.168.1.2', developerOnly: false });
    status.receiverUrls.push('https://192.168.1.2:8443/');
    await showFixture(status);
    await cdp.js(`document.querySelector('#drawer-open').click();document.querySelector('#address-network').value='1';document.querySelector('#address-network').dispatchEvent(new Event('change'));new Promise(r=>setTimeout(r,2100))`);
    expect(await cdp.js(`document.querySelector('[data-address="1"]').hidden`)).toBe(false);
    expect(await cdp.js(`document.querySelector('[data-address="0"]').hidden`)).toBe(true);
    await cdp.js(`document.querySelector('[data-detail="CAR_PAGE"]').click()`);
    expect(await cdp.js(`document.querySelector('dialog:last-of-type').innerText`)).toContain('https://192.168.1.2:8443/');
    expect(await cdp.js(`!!document.querySelector('dialog:last-of-type [data-recheck]')`)).toBe(true);
    await cdp.js(`document.querySelector('dialog:last-of-type [data-close]').click();document.querySelector('#drawer-close').click()`);
    await showFixture(linuxStatusFixtures['iphone-failure']);
    await cdp.js(`document.querySelector('#drawer-open').click();document.querySelector('[data-detail="IPHONE_BLUETOOTH"]').click()`);
    expect(await cdp.js(`document.querySelector('dialog:last-of-type').innerText`)).toContain('rfcomm_timeout');
    expect(await cdp.js(`!!document.querySelector('dialog:last-of-type time')`)).toBe(true);
    await screenshot('step-details');
    expect(await cdp.js(`!!document.querySelector('dialog:last-of-type [data-action]')`)).toBe(true);
    expect(await cdp.js('window.renderErrors')).toEqual([]);
  }, 15000);
  it('completes the guided setup through the shared fields and authenticated start route', async () => {
    await showFixture(linuxStatusFixtures.ready, 'setup/car');
    for (const step of setupSteps.slice(0, -1)) {
      expect(await cdp.js(`location.hash`)).toBe('#setup/' + step);
      await cdp.js(`document.querySelector('#setup-next').click();new Promise(r=>setTimeout(r,80))`);
    }
    expect(await cdp.js('location.hash')).toBe('#setup/start');
    await screenshot('guided-setup');
    await cdp.js(`document.querySelector('#setup-next').click();new Promise(r=>setTimeout(r,80))`);
    expect(await cdp.js('window.adminCalls')).toEqual(expect.arrayContaining(['/api/v0/setup/complete', '/api/v0/actions/start']));
    expect(await cdp.js('location.hash')).toBe('#home');
    expect(await cdp.js('window.renderErrors')).toEqual([]);
  }, 10000);
  it('shows the five-second ring and cancels the real mock arrival with Not now', async () => {
    if (script) { await cdp.call('Page.removeScriptToEvaluateOnNewDocument', { identifier: script }); script = ''; }
    await cdp.call('Page.navigate', { url: preview + '?platform=linux&state=arrival#home' });
    await cdp.js(`new Promise(r=>setTimeout(r,800))`);
    expect(await cdp.js(`document.querySelector('#arrival-ring').hasAttribute('hidden')`)).toBe(false);
    expect(await cdp.js(`document.querySelector('#arrival-ring rect').getBoundingClientRect().width`)).toBeGreaterThan(250);
    expect(await cdp.js(`Number(document.querySelector('#arrival-ring rect').getAttribute('rx'))`)).toBe(await cdp.js(`document.querySelector('#stage-primary').clientHeight / 2 - 4`));
    await screenshot('arrival');
    expect(await cdp.js(`document.querySelector('#stage-secondary').textContent`)).toBe('Not now');
    await cdp.js(`document.querySelector('#stage-secondary').click();new Promise(r=>setTimeout(r,5200))`);
    expect(await cdp.js(`fetch('/api/v0/status').then(r=>r.json()).then(s=>[s.running,s.pathInput.startup.arrival.pending,s.autoStartPaused])`)).toEqual([false, false, true]);
    expect(await cdp.js(`document.querySelector('#arrival-ring').hasAttribute('hidden')`)).toBe(true);
  }, 12000);
  it('refreshes the live iPhone picker after Pair new and disposes the sheet', async () => {
    await cdp.call('Page.navigate', { url: preview + '?platform=linux&state=pairing#settings/connection' });
    await cdp.js(`new Promise(resolve=>{const timer=setInterval(()=>{if(document.querySelector('#field-iphone')){clearInterval(timer);resolve(true)}},10);setTimeout(()=>{clearInterval(timer);resolve(false)},5000)})`);
    expect(await cdp.js(`document.body.innerText`)).toContain('iPhone');
    await cdp.js(`document.querySelector('#field-iphone').click();new Promise(r=>setTimeout(r,100))`);
    expect(await cdp.js(`document.querySelector('dialog').innerText`)).toContain('Settings → Bluetooth');
    await cdp.js(`document.querySelector('#phone-pair').click();new Promise(r=>setTimeout(r,2200));`);
    await cdp.js(`document.querySelector('#phone-refresh').click();new Promise(r=>setTimeout(r,100))`);
    expect(await cdp.js(`document.querySelector('#phone-list').innerText`)).toContain('New iPhone');
    await screenshot('iphone-picker');
    await cdp.js(`document.querySelector('[data-phone-cancel]').click()`);
    expect(await cdp.js(`document.querySelector('#phone-list')`)).toBeNull();
  }, 10000);

});
