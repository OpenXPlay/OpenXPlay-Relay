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
import { translate } from '../src/i18n';
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
  return `window.fixtureToken=${JSON.stringify(token)}; localStorage.setItem('openxplay.language', ${JSON.stringify(language)}); window.renderErrors = []; window.adminWrites = []; window.adminCalls = []; window.addEventListener('error', e => window.renderErrors.push(e.message)); window.addEventListener('unhandledrejection', e => window.renderErrors.push(String(e.reason))); const fixtureStatus = ${JSON.stringify(status)}, fixtureSettings = ${JSON.stringify(settings)}; window.fixtureStatus=fixtureStatus;window.fixtureSettings=fixtureSettings;window.fixtureSession={state:'ready',defaultPassword:false};window.fixturePhones=[];window.fetch = async (url, init = {}) => { const path = String(url).split('?')[0]; if(init.method && init.method !== 'GET') window.adminCalls.push(path); let data = {}; let code = 200; if (path.endsWith('/session')) data = window.fixtureSession; else if (path.endsWith('/settings')) { if (init.method === 'PUT') { window.adminWrites.push(JSON.parse(init.body)); Object.assign(fixtureSettings.values, JSON.parse(init.body)); data={ok:true,applied:window.fixtureApplied??'now'}; if(window.fixtureApplied==='next-start')fixtureStatus.notices=['notice.nextStart']; } else data=fixtureSettings; } else if (path.endsWith('/status')) data=fixtureStatus; else if (path.endsWith('/identity')) data=fixtureStatus.identity; else if (path.endsWith('/login')) window.fixtureSession.state='ready'; else if (path.endsWith('/password')) window.fixtureSession.defaultPassword=false; else if (path.endsWith('/phones')) data=window.fixturePhones; else if (path.endsWith('/logs/receipt')) code=404; else if (path.endsWith('/carlink-trace')) data={enabled:false,running:false,status:null,files:[]}; else if (path.includes('/carlink-trace/tail')) data={lines:[]}; return new Response(JSON.stringify(data), {status:code,headers:{'Content-Type':'application/json'}}); };`;
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
  it('saves Linux USB without confirmation and keeps cable advice under the option', async () => {
    await showFixture(linuxStatusFixtures.ready, 'settings/connection', linuxSettings);
    await cdp.js(`document.querySelector('#field-transport').click(); new Promise(r=>setTimeout(r,50))`);
    expect(await cdp.js(`document.querySelector('dialog').innerText`)).toContain('USB-A');
    const advice = await cdp.js(`document.querySelector('dialog [data-option="1"]').innerText`);
    expect(advice).toContain('USB-A to Lightning');
    expect(advice).toContain('Charge-only cables do not work');
    expect(advice).not.toContain('battery');
    await cdp.js(`document.querySelector('dialog [data-option="1"]').click(); new Promise(r=>setTimeout(r,600))`);
    expect(await cdp.js('window.adminWrites.at(-1)')).toEqual({ transport: 'usb' });
    expect(await cdp.js(`document.querySelector('dialog[open]') !== null`)).toBe(false);
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

  it('keeps text within a 360 px phone at 130% and saves Linux pictures without a confirmation', async () => {
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
    await until(`document.querySelectorAll('dialog[open]').length === 0`);
    expect(await cdp.js('document.querySelectorAll("dialog[open]").length')).toBe(0);
    expect(await cdp.js('history.state?.oxpSheet ?? null')).toBeNull();
    expect(await cdp.js('window.renderErrors')).toEqual([]);
  }, 10000);

  async function screenshot(name: string) {
    if (!process.env.ADMIN_SB_SCREENSHOTS && !process.env.ADMIN_W14_SCREENSHOTS) return;
    const shot = await cdp.call('Page.captureScreenshot', { format: 'png' });
    writeFileSync(`${process.env.ADMIN_SB_SCREENSHOTS ?? process.env.ADMIN_W14_SCREENSHOTS}/${name}.png`, Buffer.from(shot.data, 'base64'));
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

  it('coordinates FIFO, stale prompts, owner disposal, focus, children and browser Back', async () => {
    await showFixture(linuxStatusFixtures.ready, 'settings/access/admin');
    await cdp.js(`(async()=>{window.surfaces = await import('/src/dom.ts'); window.queueTest = {}; const trigger=document.querySelector('#admin-reset');trigger.focus(); window.addSheet=(name, options={})=>{const d=document.createElement('dialog');d.id='test-'+name;d.innerHTML='<input><button>Close</button>';document.body.append(d);queueTest[name]={d,h:surfaces.openSurface(d,{parent:null,...options})};}; addSheet('first',{owner:'test'}); addSheet('expired',{owner:'test',expiresAt:Date.now()-1});addSheet('second',{owner:'test'});})()`);
    expect(await cdp.js(`document.querySelectorAll('dialog[open]').length`)).toBe(1);
    expect(await cdp.js(`document.activeElement.id`)).toBe('test-first');
    expect(await cdp.js(`queueTest.expired.h.closed`)).toBe(true);
    await cdp.js(`surfaces.closeSurface(queueTest.first.d)`);
    await until(`queueTest.second.h.visible`);
    expect(await cdp.js(`document.querySelectorAll('dialog[open]').length`)).toBe(1);
    await cdp.js(`addSheet('child',{owner:'test',parent:queueTest.second.d});`);
    expect(await cdp.js(`queueTest.second.h.visible`)).toBe(false);
    await cdp.js('history.back()'); await until('queueTest.child.h.closed && queueTest.second.h.visible');
    expect(await cdp.js(`document.activeElement.id`)).toBe('test-second');
    await cdp.js('history.back()'); await until('queueTest.second.h.closed');
    expect(await cdp.js(`document.activeElement.id`)).toBe('admin-reset');
    await cdp.js(`addSheet('owner',{owner:'route-test'});addSheet('queued',{owner:'route-test'});surfaces.cancelSurfaces('route-test')`);
    await until(`queueTest.owner.h.closed && queueTest.queued.h.closed && !history.state?.oxpSheet`);
    expect(await cdp.js(`document.querySelectorAll('dialog[open]').length`)).toBe(0);
    expect(await cdp.js('window.renderErrors')).toEqual([]);
  });
  it('lets an urgent prompt preempt a sheet and routes plain showModal() calls through the queue', async () => {
    await showFixture(linuxStatusFixtures.ready, 'settings/access/admin');
    await cdp.js(`(async()=>{window.surfaces = await import('/src/dom.ts'); window.queueTest = {}; document.querySelector('#admin-reset').focus(); window.addSheet=(name, options={})=>{const d=document.createElement('dialog');d.id='test-'+name;d.innerHTML='<input id="input-'+name+'"><button>Close</button>';document.body.append(d);queueTest[name]={d,h:surfaces.openSurface(d,{parent:null,...options})};};})()`);
    await cdp.js(`addSheet('base',{owner:'test'}); document.querySelector('#input-base').focus(); queueTest.pin={d:document.createElement('dialog')}; queueTest.pin.d.id='test-pin'; queueTest.pin.h=surfaces.openPrompt(queueTest.pin.d,{owner:'pin',priority:surfaces.SURFACE_PRIORITY.urgent})`);
    expect(await cdp.js(`[queueTest.base.h.visible, queueTest.base.h.closed, queueTest.pin.h.visible, document.querySelectorAll('dialog[open]').length]`)).toEqual([false, false, true, 1]);
    await cdp.js(`surfaces.closeSurface(queueTest.pin.d)`); await until(`queueTest.base.h.visible && history.state?.oxpSheet === queueTest.base.d.dataset.historySheet`);
    expect(await cdp.js(`document.activeElement.id`)).toBe('input-base');
    await cdp.js(`window.plain=document.createElement('dialog'); plain.id='plain'; document.body.append(plain); plain.showModal()`);
    expect(await cdp.js(`[plain.open, plain.classList.contains('sheet'), queueTest.base.h.visible]`)).toEqual([true, true, false]);
    await cdp.js(`plain.close()`); await until(`queueTest.base.h.visible && !plain.isConnected`);
    await cdp.js(`surfaces.closeSurface(queueTest.base.d)`); await until(`!history.state?.oxpSheet && !document.querySelector('dialog[open]')`);
    expect(await cdp.js(`document.activeElement.id`)).toBe('admin-reset');
    expect(await cdp.js('window.renderErrors')).toEqual([]);
  });
  async function until(condition: string) {
    expect(await cdp.js(`new Promise(resolve=>{const end=performance.now()+4000;function check(){if(${condition})resolve(true);else if(performance.now()>end)resolve(false);else requestAnimationFrame(check)}check()})`), condition).toBe(true);
  }

  it('saves Linux pictures in place and keeps admin lockout recovery inside a destructive decision', async () => {
    await showFixture(linuxStatusFixtures['live-browser'], 'settings/picture', linuxSettings);
    await cdp.js(`window.fixtureApplied='next-start';document.querySelector('#field-frameRate').click()`);
    await until(`!!document.querySelector('dialog [data-option]')`);
    await cdp.js(`document.querySelector('dialog [data-option="1"]').click()`);
    await until(`window.adminWrites.some(w=>'frameRate' in w) && !document.querySelector('dialog[open]') && !document.querySelector('#restart-required').hidden`);
    expect(await cdp.js(`document.querySelector('#restart-required').innerText`)).toContain('Applies at the next connection');
    expect(await cdp.js(`document.querySelector('#restart-now').className`)).toBe('quiet');
    await cdp.js(`location.hash='home'`); await until(`!!document.querySelector('#drawer-open')`);
    expect(await cdp.js(`document.querySelector('[data-vehicle-tile]')`)).toBeNull();
    expect(await cdp.js(`document.querySelector('#stage-notice').innerText`)).not.toContain('screen changed');
    await cdp.js(`location.hash='settings/access/admin'`); await until(`!!document.querySelector('#admin-enabled')`);
    expect(await cdp.js(`!!document.querySelector('#admin-enabled + .switch-track')`)).toBe(true);
    expect(await cdp.js(`document.querySelector('main').innerText`)).not.toMatch(/SSH|--admin/);
    await cdp.js(`document.querySelector('#admin-enabled').click()`); await until(`!!document.querySelector('dialog[open] [data-confirm]')`);
    expect(await cdp.js(`document.querySelector('dialog [data-confirm]').classList.contains('destructive')`)).toBe(true);
    expect(await cdp.js(`document.querySelector('dialog [data-close]').classList.contains('primary')`)).toBe(true);
    await cdp.js(`document.querySelector('dialog [data-close]').click()`); await until(`!document.querySelector('dialog[open]')`);
    expect(await cdp.js(`document.querySelector('#admin-enabled').checked`)).toBe(true);
  });
  it('Next saves open guide drafts, focuses required rows and clears their errors on change', async () => {
    const settings=structuredClone(linuxSettings); Object.assign(settings.values,{carBrand:'other',carBrandText:'',carModel:'other',carModelText:'',carYear:'other',carYearText:''});
    await showFixture(linuxStatusFixtures.ready,'setup/car',settings);
    expect(await cdp.js(`document.querySelectorAll('.guide-blocker').length`)).toBe(0);
    await cdp.js(`document.querySelector('#setup-next').click()`); await until(`!!document.querySelector('.guide-blocker')`);
    expect(await cdp.js(`document.activeElement.dataset.field`)).toBe('carBrandText');
    await cdp.js(`for(const [key,value] of [['carBrandText','My brand'],['carModelText','My model'],['carYearText','2026']]){const input=document.querySelector('#field-'+key);input.value=value;input.dispatchEvent(new Event('input'));}`);
    expect(await cdp.js(`document.querySelectorAll('.guide-blocker').length`)).toBe(0);
    await cdp.js(`document.querySelector('#setup-next').click()`); await until(`location.hash==='#setup/output'`);
    expect(await cdp.js(`window.adminWrites`)).toEqual(expect.arrayContaining([{carBrandText:'My brand'},{carModelText:'My model'},{carYearText:'2026'}]));
    await cdp.js(`document.querySelector('.back').click()`); await until(`location.hash==='#setup/car'`);
    await cdp.js(`document.querySelector('#field-carBrand').click()`); await until(`!!document.querySelector('dialog[open]')`);
    expect(await cdp.js(`document.querySelector('dialog').innerText`)).not.toMatch(/^Choose$/m);
    expect(await cdp.js('window.renderErrors')).toEqual([]);
  });
  it('offers one default-password sheet per sign-in, one skip and a persistent drawer row', async () => {
    await freshPage();
    const source=renderSource(linuxStatusFixtures.ready,linuxSettings,'en','signin')+`window.fixtureSession={state:'login',defaultPassword:true};`;
    script=(await cdp.call('Page.addScriptToEvaluateOnNewDocument',{source})).identifier;
    await cdp.call('Page.navigate',{url:preview+'?signin#home'}); await until(`!!document.querySelector('#auth-form')`);
    // The default password is public; a relay that still uses it says so on its sign-in page.
    expect(await cdp.js('document.querySelector("h1").innerText')).toBe(translate('en', 'ui.login'));
    expect(await cdp.js('document.body.innerText')).toContain(translate('en', 'ui.firstTime'));
    await cdp.js(`document.querySelector('#password').value='openxplay';document.querySelector('#auth-form').requestSubmit()`);
    await until(`!!document.querySelector('[data-password-skip]')`);
    expect(await cdp.js(`document.querySelectorAll('[data-password-skip]').length`)).toBe(1);
    await cdp.js(`document.querySelector('[data-password-skip]').click()`); await until(`!document.querySelector('dialog[open]')`);
    expect(await cdp.js('document.querySelector("main").innerText')).not.toContain('Set a password');
    await cdp.js(`document.querySelector('#drawer-open').click()`); await until(`!!document.querySelector('#stage-drawer[open]')`);
    expect(await cdp.js(`[...document.querySelectorAll('#drawer-details a')].filter(a=>a.textContent.includes('Set a password')).length`)).toBe(1);
    expect(await cdp.js('window.renderErrors')).toEqual([]);
  });
  it('renders settings and guide phone screenshots in both languages and themes', async () => {
    for(const language of ['en','zh-CN']) {
      await freshPage(); script=(await cdp.call('Page.addScriptToEvaluateOnNewDocument',{source:renderSource(linuxStatusFixtures.ready,linuxSettings,language,'screenshots')})).identifier;
      await cdp.call('Page.navigate',{url:preview+'?screenshots#settings/car'}); await until(`!!document.querySelector('.settings-page')`);
      for(const theme of ['light','dark']) {
        await cdp.call('Emulation.setEmulatedMedia',{features:[{name:'prefers-color-scheme',value:theme},{name:'prefers-reduced-motion',value:'reduce'}]});
        for(const route of ['settings/car','setup/car','setup/prepare','settings/access/admin','settings/connection/network','settings/picture']) {
          await cdp.js(`location.hash=${JSON.stringify(route)}`); await until(`location.hash==='#${route}' && !!document.querySelector('.settings-page')`);
          expect(await cdp.js('document.documentElement.scrollWidth')).toBeLessThanOrEqual(390);
          await screenshot('sb-'+language+'-'+theme+'-'+route.replaceAll('/','-'));
        }
      }
    }
    expect(await cdp.js('window.renderErrors')).toEqual([]);
  },20000);

  it('marks guide blockers, keeps drawer rows grouped and renders decision sheets at phone width', async () => {
    const status=structuredClone(linuxStatusFixtures.ready) as Status; status.pathInput!.iphone!.pick='NONE'; status.running=false;
    const settings=structuredClone(linuxSettings); Object.assign(settings.values,{carBrand:'zeekr',carModel:'',sharedWifiNetworks:[{ssid:'Home Wi-Fi',hasPassword:true}]});
    for(const language of ['en','zh-CN']) for(const theme of ['light','dark']) {
      const name=(view: string) => `sb-${language}-${theme}-${view}`;
      const fits=async (view: string) => { expect(await cdp.js('document.documentElement.scrollWidth'), view).toBeLessThanOrEqual(390); await screenshot(name(view)); };
      await freshPage();
      await cdp.call('Emulation.setEmulatedMedia',{features:[{name:'prefers-color-scheme',value:theme},{name:'prefers-reduced-motion',value:'reduce'}]});
      script=(await cdp.call('Page.addScriptToEvaluateOnNewDocument',{source:renderSource(status,structuredClone(settings),language,'sheets')+`window.fixtureSession={state:'login',defaultPassword:true};`})).identifier;
      await cdp.call('Page.navigate',{url:preview+'?sheets#home'}); await until(`!!document.querySelector('#auth-form')`); await fits('login');
      await cdp.js(`document.querySelector('#password').value='openxplay';document.querySelector('#auth-form').requestSubmit()`);
      await until(`!!document.querySelector('dialog[open] [data-password-skip]')`); await fits('password-sheet');
      await cdp.js(`document.querySelector('[data-password-skip]').click()`); await until(`!document.querySelector('dialog[open]')`);
      await cdp.js(`location.hash='setup/car'`); await until(`!!document.querySelector('#setup-next')`);
      expect(await cdp.js(`!!document.querySelector('#car-notes a[href="#settings/access/public-name"]')`)).toBe(true);
      await cdp.js(`document.querySelector('#setup-next').click()`); await until(`!!document.querySelector('.guide-blocker')`);
      expect(await cdp.js(`document.querySelector('.guide-blocker .field-result').textContent`)).toBe(translate(language as 'en', 'car.modelRequired'));
      await fits('setup-car-blocker');
      await cdp.js(`location.hash='setup/prepare'`); await until(`!!document.querySelector('#prepare-IPHONE_PAIRED')`);
      await cdp.js(`document.querySelector('#setup-next').click()`); await until(`document.activeElement?.id==='prepare-IPHONE_PAIRED'`);
      await cdp.js(`window.fixtureStatus.pathInput.iphone.pick='NONE'`); await new Promise(resolve => setTimeout(resolve, 1200));
      expect(await cdp.js(`document.querySelectorAll('.guide-blocker').length`)).toBe(1);
      await fits('setup-prepare-blocker');
      await cdp.js(`location.hash='setup/start'`); await until(`!!document.querySelector('#prepare-IPHONE_PAIRED')`);
      expect(await cdp.js(`!!document.querySelector('[data-key="prepare-ready"]')`)).toBe(false);
      await fits('setup-start');
      await cdp.js(`location.hash='home'`); await until(`!!document.querySelector('#drawer-open')`);
      await cdp.js(`document.querySelector('#drawer-open').click()`); await until(`!!document.querySelector('#stage-drawer[open] #drawer-details')`);
      expect(await cdp.js(`[...document.querySelectorAll('#drawer-details .inset-group')].filter(group=>group.querySelector('a[href="#settings/access/admin"]')).length`)).toBe(1);
      await cdp.js(`document.querySelector('#drawer-details a[href="#settings/access/admin"]').scrollIntoView({block:'center'})`); await fits('home-drawer');
      await cdp.js(`document.querySelector('#drawer-close').click()`); await until(`!document.querySelector('dialog[open]')`);
      await cdp.js(`location.hash='settings/access/admin'`); await until(`!!document.querySelector('#admin-enabled')`);
      await cdp.js(`document.querySelector('#admin-enabled').click()`); await until(`!!document.querySelector('dialog[open] [data-confirm]')`); await fits('admin-confirm');
      await cdp.js(`document.querySelector('dialog [data-close]').click()`); await until(`!document.querySelector('dialog[open]')`);
      await cdp.js(`location.hash='settings/connection/network'`); await until(`!!document.querySelector('#field-networkMode')`);
      await cdp.js(`document.querySelector('#field-networkMode').click()`); await until(`!!document.querySelector('dialog[open] [data-option]')`);
      await cdp.js(`[...document.querySelectorAll('dialog[open] [data-option]')].find(button=>button.textContent.includes(${JSON.stringify(translate(language as 'en', 'option.networkMode.shared-wifi'))})).click()`);
      await until(`!!document.querySelector('dialog[open] [data-confirm]')`);
      // Joining shared Wi-Fi drops the hotspot address: the way back is the relay's stable name, once, with a copy action.
      expect(await cdp.js(`document.querySelector('dialog[open] .address-row [data-copy]').dataset.copy`)).toContain(status.host);
      expect(await cdp.js(`document.querySelectorAll('dialog[open] [data-copy]').length`)).toBe(1);
      await fits('network-switch');
      await cdp.js(`document.querySelector('dialog[open] [data-close], dialog[open] .quiet').click()`); await until(`!document.querySelector('dialog[open]')`);
    }
    expect(await cdp.js('window.renderErrors')).toEqual([]);
  }, 120000);

  it('keeps a single vehicle switch on its page and limits its output link to Carlink', async () => {
    const status=structuredClone(linuxStatusFixtures['live-carlink']);
    await showFixture(status,'settings/output',carlinkSettings);
    expect(await cdp.js(`!!document.querySelector('a[href="#settings/vehicle"]')`)).toBe(true);
    expect(await cdp.js(`document.querySelector('#field-vehicleInformation')`)).toBeNull();
    await cdp.js(`location.hash='settings/vehicle'`); await until(`!!document.querySelector('#field-vehicleInformation')`);
    expect(await cdp.js(`document.querySelectorAll('#field-vehicleInformation').length`)).toBe(1);
    expect(await cdp.js(`document.querySelector('[data-vehicle-enable]')`)).toBeNull();
    await showFixture(linuxStatusFixtures.ready,'settings/output',linuxSettings);
    expect(await cdp.js(`document.querySelector('a[href="#settings/vehicle"]')`)).toBeNull();
  });
  it('lists only phones with Automatic in the picker and puts view areas after screen details', async () => {
    await showFixture(linuxStatusFixtures.ready,'settings/connection',linuxSettings);
    await cdp.js(`window.fixturePhones=[{id:'phone',name:'My iPhone',kind:'phone',connected:true,selected:false},{id:'car',name:'Car audio',kind:'car'},{id:'other',name:'Headphones',kind:'other'}];document.querySelector('#field-iphone').click()`);
    await until(`document.querySelectorAll('#phone-list button').length===2`);
    expect(await cdp.js(`document.querySelector('#phone-list').innerText`)).toContain('My iPhone');
    expect(await cdp.js(`document.querySelector('#phone-list').innerText`)).not.toMatch(/Car audio|Headphones/);
    await cdp.js(`document.querySelector('[data-phone-cancel]').click()`); await until(`!document.querySelector('dialog[open]')`);
    await cdp.js(`location.hash='settings/car'`); await until(`!!document.querySelector('#field-carPlayViewAreas')`);
    expect(await cdp.js(`!!(document.querySelector('#car-screen').compareDocumentPosition(document.querySelector('#field-carPlayViewAreas'))&Node.DOCUMENT_POSITION_FOLLOWING)`)).toBe(true);
    expect(await cdp.js(`document.querySelector('#field-carPlayViewAreas').closest('details').open`)).toBe(false);
  });

});
