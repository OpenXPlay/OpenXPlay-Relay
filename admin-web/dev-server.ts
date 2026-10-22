// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-15

import { validateHotspot } from './src/hotspot-settings';
import { validateSharedNetworks } from './src/shared-networks';
import type { SavedNetwork } from './src/schema';
import { randomUUID } from 'node:crypto';
import type { Plugin } from 'vite';
import type { Field, Section, Settings, Value } from './src/schema';
import type { PublicNameStatus, ReceiverReport, Status, WifiClient, Wireless } from './src/api';

function field(key: string, type: Field['type'], extra: Partial<Field> = {}): Field {
  return { key, type, labelKey: `field.${key}`, ...extra };
}
function choice(key: string, values: string[], extra: Partial<Field> = {}): Field {
  return field(key, 'choice', { options: values.map(value => ({ value, labelKey: `option.${key}.${value}` })), ...extra });
}
const when = (key: string, ...equals: Value[]) => ({ key, equals });
// Trusted HTTPS as the Linux relay describes it (public-name.json); the token never comes back.
const PUBLIC_NAME_OFF = { publicNameEnabled: false, publicNameProvider: '', publicName: '', publicNameToken: '', publicNameTokenSet: false, publicNameAgreeTerms: false, publicNameProviders: { duckdns: { name: '', tokenSet: false }, dynv6: { name: '', tokenSet: false } } };
function publicNameSection(): Section {
  const shown = { visibleWhen: when('publicNameEnabled', true) };
  return { id: 'publicName', titleKey: 'publicName.title', fields: [
    field('publicNameEnabled', 'bool', { labelKey: 'publicName.enabled', hintKey: 'publicName.summary' }),
    choice('publicNameProvider', ['duckdns', 'dynv6'], { labelKey: 'publicName.provider', ...shown }),
    field('publicName', 'text', { labelKey: 'publicName.name', hintKey: 'publicName.nameHint', ...shown }),
    field('publicNameToken', 'secret', { labelKey: 'publicName.token', hintKey: 'publicName.tokenHint', ...shown }),
    field('publicNameAgreeTerms', 'bool', { labelKey: 'publicName.terms', ...shown }),
  ] };
}
function mockWireless(values: Settings['values']): Wireless {
  const id = String(values.devicePreset === 'auto' ? 'phicomm-n1' : values.devicePreset);
  const labels: Record<string, string> = { 'phicomm-n1': 'Phicomm N1', msm8916: 'Qualcomm Snapdragon 410 (MSM8916)', generic: 'Generic Linux device' };
  const configuredBand = values.hotspotBand as Wireless['hotspot']['configuredBand'];
  const band = configuredBand === 'auto' ? id === 'msm8916' ? '2.4' : '5' : configuredBand;
  const configuredChannel = Number(values.hotspotChannel);
  const ap5Channels = [36, 40, 44, 48, 149, 153, 157, 161, 165];
  const inBand = band === '2.4' ? configuredChannel >= 1 && configuredChannel <= 14 : configuredChannel >= 32;
  const capable = band === '2.4' ? configuredChannel <= 11 : ap5Channels.includes(configuredChannel);
  return {
    preset: { id, label: labels[id], source: values.devicePreset === 'auto' ? 'detected' : 'settings', detected: 'phicomm-n1' },
    mode: 'concurrent', station24: true, ap24: true, station5: true, ap5: true, apStation: true, apStationChannels: 1, p2pClient: true, ap5Channels,
    hotspot: { band, autoBand: id === 'msm8916' ? '2.4' : '5', channel: inBand && capable ? configuredChannel : band === '2.4' ? 6 : 36, configuredBand, configuredChannel,
      reason: configuredChannel && !(inBand && capable) ? inBand ? 'channel-not-ap-capable' : 'channel-not-in-band' : null },
  };
}
/** The mock's trusted-HTTPS status: ready once complete; `fail.duckdns.org` shows an error. */
export function mockPublicName(values: Settings['values'], now = Date.now() / 1000): PublicNameStatus {
  if (values.publicNameEnabled !== true) return { state: 'off' };
  const name = String(values.publicName ?? '');
  if (!values.publicNameProvider || !name || values.publicNameTokenSet !== true || values.publicNameAgreeTerms !== true) return { state: 'incomplete', ...(name ? { name } : {}) };
  if (name.startsWith('fail.')) return { state: 'error', name, address: '192.168.1.78', error: 'acme-rejected', errorDetail: 'Incorrect TXT record found at _acme-challenge.' + name, nextAttempt: Math.floor(now) + 300 };
  return { state: 'ready', name, address: '192.168.1.78', notAfter: Math.floor(now) + 90 * 86400, trustedName: name };
}
export function mockSettings(linux = false): Settings {
  if (linux) return { schema: [
    { id: 'connection', titleKey: 'section.connection', fields: [choice('devicePreset', ['auto', 'msm8916', 'phicomm-n1', 'generic']), choice('hotspotBand', ['auto', '2.4', '5'], { visibleWhen: when('networkMode', 'device-hotspot') }), field('iphone', 'phone'), choice('output', ['web', 'carlink', 'both']), field('requirePairingCode', 'bool'), field('autoStart', 'bool', { hintKey: 'hint.field.linuxAutoStart' })] },
    { id: 'carlink', titleKey: 'section.carlink', fields: [
      field('carlinkName', 'text', { visibleWhen: when('output', 'carlink', 'both') }),
      field('carlinkModel', 'text', { visibleWhen: when('output', 'carlink', 'both') }),
      field('carlinkAutoReconnect', 'bool', { hintKey: 'hint.field.carlinkAutoReconnect', visibleWhen: when('output', 'carlink', 'both') }),
      field('carlinkReconnectOnResize', 'bool', { hintKey: 'hint.field.carlinkReconnectOnResize', visibleWhen: when('output', 'carlink', 'both') }),
      choice('carlinkHevc', ['auto', 'off'], { hintKey: 'hint.field.carlinkHevc', visibleWhen: when('output', 'carlink', 'both') }),
      choice('videoStats', ['off', '5', '15']), field('carPlayViewAreas', 'bool'),
      field('receiverPort', 'number', { min: 1, max: 65535 }),
    ] },
    { id: 'audio', titleKey: 'section.audio', fields: [field('carBluetoothAudio', 'bool', { hintKey: 'hint.field.carBluetoothAudio' })] },
    publicNameSection(),
    { id: 'plugins', titleKey: 'section.plugins', fields: [field('plugins', 'bool', { hintKey: 'hint.plugins' }), field('pluginsEnabled', 'plugins')] },
    { id: 'wifi', titleKey: 'section.wifi', fields: [] },
    { id: 'network', titleKey: 'section.network', fields: [choice('networkMode', ['shared-wifi', 'device-hotspot'], { options: ['shared-wifi', 'device-hotspot'].map(value => ({ value, labelKey: `option.networkMode.${value}`, hintKey: `hint.option.networkMode.${value}`, illustration: value as 'shared-wifi' | 'device-hotspot' })) }), field('sharedWifiNetworks', 'networks')] },
    { id: 'advanced', titleKey: 'section.advanced', fields: [field('wifiInterface', 'text'), field('hotspotChannel', 'number', { min: 0, max: 177, labelKey: 'field.wifiChannel', hintKey: 'hint.field.hotspotChannel' }), field('rfcommChannel', 'number', { min: 1, max: 30, hintKey: 'hint.rfcommChannel' })] },
  ], installedPlugins: [{ id: 'org.openxplay.clock', name: { en: 'Clock', 'zh-CN': '时钟' }, version: '1.0.0', license: 'Apache-2.0 OR MIT', icon: null, nonCommercialAssets: false },
    { id: 'org.openxplay.pet', name: { en: 'Pet', 'zh-CN': '宠物' }, version: '1.0.0', license: '(Apache-2.0 OR MIT) AND CC-BY-NC-SA-4.0', icon: null, nonCommercialAssets: true }],
  values: { plugins: false, pluginsEnabled: [], iphone: 'auto', networkMode: 'shared-wifi', sharedWifiNetworks: [{ ssid: 'Home Wi-Fi', hasPassword: true, current: false }, { ssid: 'Car Wi-Fi', hasPassword: false, current: false }], wifiInterface: 'wlan0', hotspotChannel: 0, hotspotBand: 'auto', devicePreset: 'auto', rfcommChannel: '', output: 'web', requirePairingCode: false, carBluetoothAudio: true, autoStart: true, carlinkName: 'OpenXPlay Linux', carlinkModel: 'OpenXPlay Relay Linux', carlinkAutoReconnect: true, carlinkReconnectOnResize: false, carlinkHevc: 'auto', videoStats: 'off', carPlayViewAreas: true, receiverPort: 8443, ...PUBLIC_NAME_OFF } };
  const schema: Section[] = [
    { id: 'connection', titleKey: 'section.connection', fields: [
      field('iphone', 'phone', { hintKey: 'hint.iphone' }), choice('transport', ['wireless', 'usb']),
    ] },
    { id: 'network', titleKey: 'section.network', fields: [
      choice('networkMode', ['wifi-direct', 'phone-hotspot', 'shared-wifi'], {
        visibleWhen: when('transport', 'wireless'),
        options: ['wifi-direct', 'phone-hotspot', 'shared-wifi'].map(value => ({ value, labelKey: `option.networkMode.${value}`, hintKey: `hint.option.networkMode.${value}`, illustration: value as 'wifi-direct' | 'phone-hotspot' | 'shared-wifi' })),
      }),
      field('hotspotSsid', 'text', { visibleWhen: when('networkMode', 'phone-hotspot') }),
      field('hotspotPassword', 'secret', { visibleWhen: when('networkMode', 'phone-hotspot') }),
      choice('hotspotBand', ['auto', '2.4', '5'], { visibleWhen: when('networkMode', 'phone-hotspot') }),
      field('sharedWifiNetworks', 'networks'),
    ] },
    { id: 'output', titleKey: 'section.output', fields: [
      choice('output', ['web', 'carlink', 'both']), field('requirePairingCode', 'bool', { hintKey: 'hint.requirePairingCode' }),
    ] },
    { id: 'display', titleKey: 'section.display', fields: [
      choice('displayMode', ['auto', 'custom']),
      field('displayWidth', 'number', { min: 320, max: 3840, visibleWhen: when('displayMode', 'custom') }),
      field('displayHeight', 'number', { min: 240, max: 2160, visibleWhen: when('displayMode', 'custom') }),
      field('displayDensity', 'number', { min: 80, max: 640, visibleWhen: when('displayMode', 'custom') }),
    ] },
    { id: 'startup', titleKey: 'section.startup', fields: [field('autoStart', 'bool', { hintKey: 'hint.autoStart' }), field('bootStart', 'bool')] },
    { id: 'carlink', titleKey: 'section.carlink', fields: [
      field('carlinkName', 'text', { visibleWhen: when('output', 'carlink', 'both') }),
      field('vehicleInformation', 'text', { visibleWhen: when('output', 'carlink', 'both') }),
    ] },
  ];
  return { schema, values: { iphone: 'auto', transport: 'wireless', networkMode: 'wifi-direct', hotspotSsid: 'OpenXPlay', hotspotPassword: 'openxplay123', hotspotBand: 'auto', sharedWifiNetworks: [{ ssid: 'Home Wi-Fi', hasPassword: true, current: false }, { ssid: 'Car Wi-Fi', hasPassword: false, current: false }], output: 'both', requirePairingCode: true, displayMode: 'auto', displayWidth: 1280, displayHeight: 720, displayDensity: 160, autoStart: true, bootStart: false, carlinkName: 'OpenXPlay Relay', vehicleInformation: '' } };
}
// A Chromium head unit with hardware H.264 and software H.265, plus a few seconds of stats.
export function mockReceivers(now = Date.now()): ReceiverReport[] {
  const decode = (codec: string, efficient: boolean) => ({ codec, width: 1920, height: 1080, preferHardware: efficient, preferSoftware: true,
    fps30: { supported: true, smooth: true, powerEfficient: efficient }, fps60: { supported: true, smooth: efficient, powerEfficient: efficient }, mediaSource: true });
  const info = { userAgent: 'Mozilla/5.0 (Linux; Android 10; Head unit) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.6099.230 Mobile Safari/537.36',
    userAgentData: { brands: [{ brand: 'Chromium', version: '120' }], mobile: true, platform: 'Android', platformVersion: '10.0.0', fullVersionList: [{ brand: 'Not_A Brand', version: '8.0.0.0' }, { brand: 'Chromium', version: '120.0.6099.230' }] },
    screen: { width: 1920, height: 720, availWidth: 1920, availHeight: 720, colorDepth: 24, orientation: 'landscape-primary' }, devicePixelRatio: 1, innerWidth: 1920, innerHeight: 720, refreshHz: 59.94,
    webgl: { available: true, vendor: 'WebKit', renderer: 'WebKit WebGL', unmaskedVendor: 'Qualcomm', unmaskedRenderer: 'ANGLE (Qualcomm, Adreno (TM) 640, OpenGL ES 3.2)', maxTextureSize: 16384 },
    videoDecoder: true, videoDecode: [decode('avc1.640028', true), decode('hvc1.1.6.L120.B0', false)],
    audio: { sampleRate: 48000, baseLatency: 0.01, outputLatency: 0.04, audioWorklet: true }, audioDecoder: [{ codec: 'mp4a.40.2', supported: true }, { codec: 'opus', supported: true }],
    network: { type: 'wifi', effectiveType: '4g', downlink: 10, rtt: 20, saveData: false }, memory: { jsHeapSizeLimit: 1073741824, totalJSHeapSize: 33554432, usedJSHeapSize: 25165824 } };
  const stats = Array.from({ length: 10 }, (_, i) => ({ type: 'stats', screen: 0, wsBytesPerSecond: 480000 + ((now / 1000 + i) % 7) * 10000, videoFramesPerSecond: 60, decoderOutputPerSecond: 60,
    decodeLatencyP50Ms: 4, decodeLatencyP95Ms: 9 + (i % 3), decodeLatencyMaxMs: 14 + i, renderedFps: 59.8, droppedFrames: i === 6 ? 2 : 0,
    eventLoopLagAvgMs: 1, eventLoopLagMaxMs: 6 + (i % 4), longTaskCount: i === 3 ? 1 : 0, longTaskTotalMs: i === 3 ? 62 : 0, usedJSHeapSize: 25165824 }));
  return [{ client: '1', idleSeconds: 0.4, info, stats }];
}
export function previewStatus(base: Status, state: string): Status {
  const status: Status = { ...base, wifiControl: base.platform !== 'android', hotspot: { ssid: 'OpenXPlay', password: 'openxplay123', ip: null, active: false }, ...(base.platform !== 'android' ? { wifiProvisioning: true } : {}), wifiClient: { ssid: 'Car Wi-Fi', ip: '192.168.1.78', signal: 82, setupHotspot: false },
    hostAddresses: [{ interface: 'wlan0', ipv4: '192.168.1.78', developerOnly: false }, { interface: 'usb0', ipv4: '192.168.42.1', developerOnly: true }],
    receiverPort: 8443, adminPort: 6975, pairingAvailable: true, pairingDeadline: null, pairingCancelable: base.platform !== 'android',
    pairedDevices: [{ name: "Riko's iPhone", connected: true, iphone: true }, { name: 'Car audio', connected: false, iphone: false }],
    video: { codec: 'H264', reason: 'hardware-hevc-unavailable', codecString: 'avc1.640028', width: 1920, height: 720 },
    state: 'active', running: true, iphone: { name: 'iPhone', reason: 'connected' }, network: { mode: 'shared-wifi', wifi: 'Car Wi-Fi' },
    output: 'both', carlink: { state: 'connected', detail: '', trusted: true }, notices: [], screenConnected: true };
  if (state === 'hotspot') { status.platform = 'linux'; status.wifiControl = true; status.network = { mode: 'device-hotspot', wifi: null }; status.wifiClient = { ssid: null, ip: null, signal: null, setupHotspot: false }; status.hotspot = { ssid: 'OpenXPlay', password: 'openxplay123', ip: '10.42.0.1', active: true }; status.hostAddresses![0].ipv4 = '10.42.0.1'; }
  if (state === 'fresh') { status.hotspot = { ssid: 'OpenXPlay', password: 'openxplay123', ip: '10.78.0.1', active: true }; status.wifiClient = { ssid: null, ip: '10.78.0.1', signal: null, setupHotspot: true }; status.network.wifi = null; status.hostAddresses![0].ipv4 = '10.78.0.1'; }
  if (state === 'fresh' || state === 'no-iphone') { status.iphone = { name: null, reason: 'none' }; status.state = 'waiting'; status.pairedDevices = []; status.video = { codec: 'H264', reason: 'no-main-screen-output' }; }
  if (base.platform === 'linux') status.bluetoothGuard = { state: 'ok', attempts: 0, lastRecovery: null, lastError: null, address: '00:1A:7D:DA:71:13' };
  if (state === 'bluetooth') status.bluetoothGuard = { state: 'recovering', attempts: 1, lastRecovery: '2026-10-16T08:00:00Z', lastError: 'hci0 has no address', address: null };
  if (state === 'carlink-unpaired') status.carlink = { state: 'unpaired', detail: 'carlink.unpairedHint', trusted: false };
  if (state === 'error') { status.state = 'reconnecting'; status.notices = ['wifi.joinFailed.timeout']; status.carlink = { state: 'error', detail: '', trusted: false, lastError: 'error.request' }; }
  return status;
}
export function mockApi(linux = false): Plugin {
  const preview = process.env.DIRELAY_PREVIEW_STATES === '1';
  let outageUntil = 0;
  let networkChanged = false;
  let previewInitialized = false;
  let pairingDeadline: number | null = null;
  let password = 'openxplay';
  let defaultPassword = true;
  let wifiClient: WifiClient = { ssid: null, ip: '10.78.0.1', signal: null, setupHotspot: true };
  let notices: string[] = [];
  // Use password openxplay for secured networks; named entries simulate join failures.
  const networks = [
    { ssid: 'Home Wi-Fi', signal: 82, secure: true, known: true },
    { ssid: 'Cafe (open)', signal: 64, secure: false, known: false },
    { ssid: 'Join failure: not found', signal: 42, secure: false, known: false },
    { ssid: 'Join failure: timeout', signal: 28, secure: false, known: false },
  ];
  const sessions = new Set<string>();
  const attempts = new Map<string, { count: number; until: number }>();
  const settings = mockSettings(linux);
  const publicProviders = new Map<string, { name: string; token: string }>();
  const currentSsid = () => String(settings.values.networkMode) === 'shared-wifi' ? wifiClient.ssid : null;
  const redactNetworks = () => {
    settings.values.sharedWifiNetworks = (settings.values.sharedWifiNetworks as SavedNetwork[]).map(item => ({ ssid: item.ssid, hasPassword: !!sharedPasswords.get(item.ssid), current: item.ssid === currentSsid() }));
  };
  let sharedPasswords = new Map<string, string>([["Home Wi-Fi", "openxplay"], ["Car Wi-Fi", ""]]);
  let running = false; let paused = false; let started = 0;
  const phones = [
    { id: 'iphone-riko', name: "Riko's iPhone", kind: 'phone', connected: true, likelyIphone: true, selected: false },
    { id: 'car', name: 'Car audio', kind: 'car', connected: false, likelyIphone: false, selected: false },
    { id: 'headphones', name: 'Headphones', kind: 'other', connected: false, likelyIphone: false, selected: false },
  ];
  return { name: 'relay-mock-api', apply: 'serve', configureServer(server) {
    server.middlewares.use(async (req, res, next) => {
      const url = new URL(req.url ?? '/', 'http://localhost');
      if (!url.pathname.startsWith('/api/v0/')) return next();
      const path = url.pathname.slice('/api/v0'.length);
      const previewState = new URL(req.headers.referer ?? '/', 'http://localhost').searchParams.get('state') ?? 'working';
      if (preview && !previewInitialized) {
        previewInitialized = true;
        settings.values.networkMode = previewState === 'hotspot' ? 'device-hotspot' : 'shared-wifi';
        wifiClient = previewState === 'fresh' || previewState === 'hotspot' ? { ssid: null, ip: null, signal: null, setupHotspot: previewState === 'fresh' } : { ssid: 'Car Wi-Fi', ip: '192.168.1.78', signal: 82, setupHotspot: false };
        if (previewState === 'fresh') { settings.values.sharedWifiNetworks = []; sharedPasswords.clear(); }
        else settings.values.sharedWifiNetworks = [{ ssid: 'Home Wi-Fi', hasPassword: true, current: false }, { ssid: 'Car Wi-Fi', hasPassword: false, current: previewState !== 'hotspot' }];
      }
      const method = req.method ?? 'GET';
      const send = (code: number, data: unknown) => {
        res.statusCode = code; res.setHeader('Content-Type', 'application/json'); res.setHeader('Cache-Control', 'no-store'); res.end(JSON.stringify(data));
      };
      const token = /(?:^|;\s*)direlay=([^;]+)/.exec(req.headers.cookie ?? '')?.[1];
      const ready = preview || (!!token && sessions.has(token));
      if (!['/session', '/login'].includes(path) && !ready) return send(401, { error: 'password' });
      if (method !== 'GET' && (req.headers['x-openxplay-admin'] !== '1' || !req.headers['content-type']?.startsWith('application/json'))) return send(403, { error: 'error.request' });
      let body: Record<string, unknown> = {};
      if (method !== 'GET') {
        try {
          let raw = '';
          for await (const chunk of req) { raw += chunk.toString(); if (Buffer.byteLength(raw) > (linux && path === '/public-name/import' ? 16384 * 6 + 1024 : 16384)) return send(413, { error: 'error.request' }); }
          body = JSON.parse(raw || '{}');
          if (!body || Array.isArray(body) || typeof body !== 'object') throw new Error();
        } catch { return send(400, { error: 'error.request' }); }
      }
      const authenticate = () => {
        const session = randomUUID(); sessions.add(session);
        // HTTP-only loopback development cookie; the real relay uses HTTPS + Secure.
        res.setHeader('Set-Cookie', `direlay=${session}; HttpOnly; SameSite=Strict; Path=/; Max-Age=2592000`);
        send(200, { ok: true });
      };
      if (path === '/session' && method === 'GET') return send(200, { state: ready ? 'ready' : 'login', defaultPassword });
      if (path === '/login' && method === 'POST') {
        const address = req.socket.remoteAddress ?? 'local'; const now = Date.now();
        let attempt = attempts.get(address);
        if (!attempt || now >= attempt.until) { attempt = { count: 0, until: now + 60000 }; attempts.set(address, attempt); }
        if (attempt.count >= 5) { res.setHeader('Retry-After', Math.ceil((attempt.until - now) / 1000)); return send(429, { error: 'error.rateLimit' }); }
        if (password !== body.password) { attempt.count++; return send(401, { error: 'password' }); }
        attempts.delete(address); return authenticate();
      }
      if (path === '/password' && method === 'POST') {
        if (body.current !== password) return send(403, { error: 'password' });
        if (typeof body.new !== 'string' || body.new.length < 8) return send(422, { error: 'error.passwordFormat' });
        password = body.new; defaultPassword = password === 'openxplay'; return send(200, { ok: true });
      }
      if (linux && path === '/hotspot' && method === 'PUT') {
        if (typeof body.ssid !== 'string' || typeof body.password !== 'string') return send(422, { error: 'error.invalid' });
        if (body.band !== undefined && !['auto', '2.4', '5'].includes(body.band)) return send(422, { error: 'error.invalid' });
        const error = validateHotspot(body.ssid, body.password);
        if (error) return send(422, { error });
        settings.values.hotspotSsid = body.ssid; settings.values.hotspotPassword = body.password;
        if (body.band !== undefined) settings.values.hotspotBand = body.band;
        networkChanged = true;
        if (settings.values.networkMode === 'device-hotspot') outageUntil = Date.now() + 2000;
        return send(200, { ok: true });
      }
      if (linux && path === '/wifi/networks'  && method === 'GET') return send(200, networks);
      if (linux && path === '/wifi' && method === 'PUT') {
        if (typeof body.ssid !== 'string' || !body.ssid.trim() || (body.password !== undefined && typeof body.password !== 'string')) return send(422, { error: 'error.invalid' });
        const list = settings.values.sharedWifiNetworks as SavedNetwork[];
        if (!list.some(item => item.ssid === body.ssid) && list.length >= 10) return send(422, { error: 'error.networks' });
        const network = networks.find(network => network.ssid === body.ssid);
        const password = body.password ?? sharedPasswords.get(body.ssid) ?? '';
        const reason = body.ssid === 'Join failure: not found' ? 'notFound' : body.ssid === 'Join failure: timeout' ? 'timeout' : network?.secure && password !== 'openxplay' ? 'auth' : null;
        notices = reason ? [`wifi.joinFailed.${reason}`] : [];
        wifiClient = reason ? { ssid: null, ip: '10.78.0.1', signal: null, setupHotspot: true } : { ssid: body.ssid, ip: '192.168.1.78', signal: network?.signal ?? 65, setupHotspot: false };
        if (!reason) {
          if (network) network.known = true; sharedPasswords.set(body.ssid, String(password));
          const list = settings.values.sharedWifiNetworks as SavedNetwork[];
          if (!list.some(item => item.ssid === body.ssid)) list.push({ ssid: body.ssid });
          settings.values.networkMode = 'shared-wifi'; redactNetworks();
        }
        networkChanged = true;
        return send(200, { ok: true });
      }
      if (linux && path === '/actions/forget-wifi' && method === 'POST') {
        wifiClient = { ssid: null, ip: '10.78.0.1', signal: null, setupHotspot: true }; notices = [];
        settings.values.sharedWifiNetworks = []; sharedPasswords.clear(); networkChanged = true;
        networks.forEach(network => { network.known = false; });
        return send(200, { ok: true });
      }
      if (path === '/logout' && method === 'POST') {
        if (token) sessions.delete(token); res.setHeader('Set-Cookie', 'direlay=; HttpOnly; SameSite=Strict; Path=/; Max-Age=0'); return send(200, { ok: true });
      }
      if (linux && path === '/public-name/import' && method === 'POST') {
        const content = body.content;
        const errors: Record<string, string> = {};
        let config: Record<string, unknown> = {};
        if (typeof content !== 'string') errors.file = 'not-json';
        else if (Buffer.byteLength(content) > 16384) errors.file = 'too-large';
        else try {
          config = JSON.parse(content);
          if (!config || Array.isArray(config) || typeof config !== 'object') throw new Error();
        } catch { errors.file = 'not-json'; }
        if (!errors.file) {
          for (const key of Object.keys(config)) if (!['version', 'enabled', 'provider', 'name', 'token', 'agreeLetsEncryptTerms', 'acme'].includes(key)) errors[key] = 'unknown-key';
          for (const key of ['version', 'provider', 'name', 'token']) if (!(key in config)) errors[key] = 'missing';
          if ('version' in config && config.version !== 1) errors.version = 'bad-version';
          if ('provider' in config && !['duckdns', 'dynv6'].includes(String(config.provider))) errors.provider = 'invalid';
          if ('name' in config && (typeof config.name !== 'string' || !/^[a-z0-9-]+(\.[a-z0-9-]+)+$/i.test(config.name) || (config.provider === 'duckdns' && !/^[a-z0-9-]+\.duckdns\.org$/i.test(config.name)))) errors.name = 'invalid';
          if ('token' in config && (typeof config.token !== 'string' || !/^[!-~]{1,256}$/.test(config.token) || (config.provider === 'duckdns' && !/^[a-f0-9]{8}(-[a-f0-9]{4}){3}-[a-f0-9]{12}$/i.test(config.token)))) errors.token = 'invalid';
          for (const key of ['enabled', 'agreeLetsEncryptTerms']) if (key in config && typeof config[key] !== 'boolean') errors[key] = 'invalid';
          if ('acme' in config && !['production', 'staging'].includes(String(config.acme))) errors.acme = 'invalid';
        }
        if (Object.keys(errors).length) return send(422, { errors });
        const provider = String(config.provider); const name = String(config.name).toLowerCase();
        publicProviders.set(provider, { name, token: String(config.token) });
        Object.assign(settings.values, { publicNameProvider: provider, publicName: name, publicNameToken: '', publicNameTokenSet: true, publicNameEnabled: config.enabled ?? true, publicNameAgreeTerms: config.agreeLetsEncryptTerms ?? false });
        settings.values.publicNameProviders = { ...(settings.values.publicNameProviders as import('./src/schema').PublicNameProviders), [provider]: { name, tokenSet: true } };
        return send(200, { ok: true, provider, name });
      }
      if (path === '/settings' && method === 'GET') return send(200, settings);
      if (path === '/phones' && method === 'GET') return send(200, phones.map(phone => ({ ...phone, selected: settings.values.iphone === phone.id })));
      if (path === '/settings' && method === 'PUT') {
        const errors: Record<string, string> = {};
        for (const [key, value] of Object.entries(body)) {
          const field = settings.schema.flatMap(section => section.fields).find(field => field.key === key);
          if (!field) { errors[key] = 'error.invalid'; continue; }
          if (field.type === 'plugins') {
            if (!Array.isArray(value) || new Set(value).size !== value.length || value.some(id => typeof id !== 'string' || !settings.installedPlugins?.some(p => p.id === id))) errors[key] = 'error.plugins';
            continue;
          }
          if (field.type === 'networks') {
            if (!validateSharedNetworks(value) || !(value as SavedNetwork[]).every(item => Object.keys(item).every(key => ['ssid', 'password'].includes(key)))) errors[key] = 'error.invalid';
            continue;
          }
          if (linux && field.type === 'number' && value === '') continue;
          if (field.type === 'bool' ? typeof value !== 'boolean' : field.type === 'number' ? typeof value !== 'number' : typeof value !== 'string') errors[key] = 'error.invalid';
          else if (field.type === 'number' && (Number(value) < field.min! || Number(value) > field.max!)) errors[key] = 'error.range';
          else if (field.type === 'choice' && !(key === 'publicNameProvider' && value === '') && !field.options?.some(option => option.value === value)) errors[key] = 'error.invalid';
          else if (field.type === 'phone' && value !== 'auto' && !phones.some(phone => phone.id === value)) errors[key] = 'error.invalid';
          else if (key === 'hotspotPassword' && String(value).length < 8) errors[key] = 'error.invalid';
          else if (['hotspotSsid', 'carlinkName'].includes(key) && !String(value).trim()) errors[key] = 'error.required';
          else if (key === 'publicName' && value !== '' && !/^[a-z0-9-]+(\.[a-z0-9-]+)+$/i.test(String(value))) errors[key] = 'error.invalid';
        }
        const requestedNetworks = body.sharedWifiNetworks ?? settings.values.sharedWifiNetworks;
        if (body.networkMode === 'shared-wifi' && Array.isArray(requestedNetworks) && requestedNetworks.length === 0) errors.networkMode = 'error.noSavedWifi';
        if (Object.keys(errors).length) return send(422, { errors });
        if (Array.isArray(body.sharedWifiNetworks)) {
          const next = body.sharedWifiNetworks as SavedNetwork[];
          const current = currentSsid();
          const retained = next.find(item => item.ssid === current);
          if (current && (!retained || (retained.password !== undefined && retained.password !== sharedPasswords.get(current)))) {
            wifiClient = { ssid: null, ip: '10.78.0.1', signal: null, setupHotspot: true }; networkChanged = true;
          }
          sharedPasswords = new Map(next.map(item => [item.ssid, item.password ?? sharedPasswords.get(item.ssid) ?? '']));
          body.sharedWifiNetworks = next.map(item => ({ ssid: item.ssid, hasPassword: !!sharedPasswords.get(item.ssid), current: item.ssid === (preview && !networkChanged ? wifiClient.ssid : currentSsid()) }));
        }
        if (linux && typeof body.publicNameProvider === 'string' && body.publicNameProvider !== settings.values.publicNameProvider) {
          const saved = publicProviders.get(body.publicNameProvider);
          settings.values.publicName = saved?.name ?? ''; settings.values.publicNameTokenSet = !!saved?.token;
        }
        const provider = String(body.publicNameProvider ?? settings.values.publicNameProvider ?? '');
        if (linux && provider && Object.keys(body).some(key => key.startsWith('publicName'))) {
          const saved = publicProviders.get(provider);
          const setup = { name: String(body.publicName ?? settings.values.publicName ?? '').trim().toLowerCase(), token: String(body.publicNameToken || saved?.token || '') };
          publicProviders.set(provider, setup);
          settings.values.publicNameProviders = { ...(settings.values.publicNameProviders as import('./src/schema').PublicNameProviders), [provider]: { name: setup.name, tokenSet: !!setup.token } };
        }
        // An empty token keeps the saved one; a token is never stored where GET /settings returns it.
        if (typeof body.publicNameToken === 'string') { if (body.publicNameToken !== '') settings.values.publicNameTokenSet = true; body.publicNameToken = ''; }
        if (typeof body.publicName === 'string') body.publicName = body.publicName.trim().toLowerCase();
        Object.assign(settings.values, body);
        if (body.networkMode !== undefined) {
          networkChanged = true; outageUntil = Date.now() + 2000;
          const shared = body.networkMode === 'shared-wifi';
          wifiClient = shared ? { ssid: (settings.values.sharedWifiNetworks as SavedNetwork[])[0].ssid, ip: '192.168.1.78', signal: 82, setupHotspot: false } : { ssid: null, ip: null, signal: null, setupHotspot: false };
          redactNetworks();
        }
        return send(200, { ok: true, applied: body.networkMode !== undefined || body.sharedWifiNetworks !== undefined || (linux && Object.keys(body).every(key => key.startsWith('publicName') || ['plugins', 'pluginsEnabled', 'output', 'requirePairingCode', 'carlinkLink', 'carlinkAddress', 'carlinkHeadUnit', 'carlinkLocal'].includes(key))) ? 'now' : running ? 'next-start' : 'now' });
      }
      if (path === '/status' && method === 'GET') {
        if (Date.now() < outageUntil) return send(503, { error: 'error.network' });
        const phone = phones.find(phone => phone.id === settings.values.iphone) ?? phones[0];
        const status: Status = {
          platform: linux ? 'linux' : 'android', name: 'OpenXPlay Relay', version: '0.1.0', host: `openxplay-${linux ? 'linux' : 'android'}.local`, adminUrl: `https://openxplay-${linux ? 'linux' : 'android'}.local:6975/`,
          wifiControl: linux,
          hotspot: { hasPassword: !!String(settings.values.hotspotPassword ?? 'openxplay123'), ssid: String(settings.values.hotspotSsid ?? 'OpenXPlay'), password: String(settings.values.hotspotPassword ?? 'openxplay123'), ip: settings.values.networkMode !== 'shared-wifi' ? '10.42.0.1' : wifiClient.setupHotspot ? '10.78.0.1' : null, active: settings.values.networkMode !== 'shared-wifi' || wifiClient.setupHotspot === true },
          ...(linux ? { wifiProvisioning: true, wifiClient } : {}),
          receiverPort: 8443, adminPort: 6975,
          hostAddresses: [{ interface: 'wlan0', ipv4: settings.values.networkMode !== 'shared-wifi' ? '10.42.0.1' : wifiClient.ip ?? '192.168.5.171', developerOnly: false }],
          receiverUrls: linux ? ['https://openxplay-linux.local:8443/'] : ['https://192.168.5.171:8443/', 'https://openxplay-android.local:8443/'],
          pairingAvailable: true, pairingCancelable: linux, pairedDevices: phones.map(phone => ({ name: phone.name, connected: phone.connected, iphone: phone.likelyIphone })),
          video: running ? { codec: 'H265', reason: 'all-main-screen-outputs-hardware-hevc', codecString: 'hvc1.1.6.L120.B0', width: 1920, height: 720 } : { codec: 'H264', reason: 'no-main-screen-output' },
          state: !running ? 'idle' : Date.now() - started < 3000 ? 'connecting' : 'active', running, autoStartPaused: paused,
          iphone: { name: phone.name, reason: running ? 'connected' : settings.values.iphone === 'auto' ? 'onlyCandidate' : 'selected' },
          network: { mode: String(settings.values.networkMode), wifi: currentSsid() },
          output: String(settings.values.output), carlink: { state: running ? 'connected' : 'idle', detail: '' }, notices,
          ...(linux ? { publicName: mockPublicName(settings.values), wireless: mockWireless(settings.values) } : {}),
        };
        if (status.publicName?.state === 'ready') status.receiverUrls = [`https://${status.publicName.name}:8443/`, ...status.receiverUrls];
        if (linux) status.adminUrls = [...(status.publicName?.trustedName ? [`https://${status.publicName.trustedName}:6975/`] : []), `https://${status.hostAddresses![0].ipv4}:6975/`, status.adminUrl];
        const result = preview ? previewStatus(status, previewState) : status;
        if (preview && networkChanged) {
          result.network = status.network; result.hotspot = status.hotspot; result.wifiClient = wifiClient; result.hostAddresses = status.hostAddresses;
        }
        result.pairingDeadline = pairingDeadline && pairingDeadline > Date.now() / 1000 ? pairingDeadline : null;
        result.pairingRemaining = result.pairingDeadline ? Math.ceil(result.pairingDeadline - Date.now() / 1000) : null;
        if (url.searchParams.get('diagnostics') === '1') result.receivers = running || preview ? mockReceivers() : [];
        return send(200, result);
      }
      if (preview && path === '/carlink/connect' && method === 'POST') return send(200, { ok: true });
      if (path === '/actions/pair-iphone' && method === 'POST') { if (!pairingDeadline || pairingDeadline <= Date.now() / 1000) pairingDeadline = Date.now() / 1000 + 600; return send(200, { ok: true }); }
      if (linux && path === '/actions/cancel-pairing' && method === 'POST') { pairingDeadline = null; return send(200, { ok: true }); }
      if (linux && path === '/actions/public-name-renew' && method === 'POST') return send(200, { ok: true });
      if (linux && path === '/actions/public-name-remove' && method === 'POST') { publicProviders.clear(); Object.assign(settings.values, PUBLIC_NAME_OFF); return send(200, { ok: true }); }
      if (path.startsWith('/actions/') && method === 'POST') {
        const name = path.split('/').pop();
        if (!['start', 'stop', 'restart', 'regenerate-pairing-code'].includes(name!)) return send(404, { error: 'error.request' });
        if (name !== 'regenerate-pairing-code') { running = name !== 'stop'; paused = !running; started = Date.now(); }
        return send(200, { ok: true });
      }
      send(404, { error: 'error.request' });
    });
  } };
}
