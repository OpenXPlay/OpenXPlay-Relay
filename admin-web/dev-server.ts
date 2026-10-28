// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-15

import { readFileSync, readdirSync } from 'node:fs';
import { validPictureSize, normalizedSizes, picturePresets } from './src/picture-sizes';
import { mfiError } from './src/mfi';
import { validateHotspot } from './src/hotspot-settings';
import { validateSharedNetworks } from './src/shared-networks';
import type { SavedNetwork } from './src/schema';
import { qrMatrix } from './src/qr';
import { deflateSync } from 'node:zlib';
import { normalizeInput } from './src/path/model';
import { mockPathInput, mockNetwork, STAGE_STATES } from './dev-fixtures';
import { createHash, randomUUID } from 'node:crypto';
import type { Plugin } from 'vite';
import type { Field, Section, Settings, Value } from './src/schema';
import type { PublicNameStatus, ReceiverReport, Status, WifiClient, Wireless } from './src/api';

const fixtureRoot = new URL('./tests/fixtures/linux/', import.meta.url);
export const linuxSettings = JSON.parse(readFileSync(new URL('settings.json', fixtureRoot), 'utf8')) as Settings;
export const linuxStatusFixtures = Object.fromEntries(readdirSync(fixtureRoot).filter(name => name.startsWith('status-') && name.endsWith('.json')).map(name => [name.slice(7, -5), JSON.parse(readFileSync(new URL(name, fixtureRoot), 'utf8')) as Status]));
export function linuxFixtureStatus(state: string, now = Date.now()): Status {
  const alias: Record<string, string> = { live: 'live-browser', working: 'live-browser', failure: 'iphone-failure', error: 'iphone-failure' };
  const status = structuredClone(linuxStatusFixtures[alias[state] ?? state] ?? linuxStatusFixtures.ready);
  const old = status.pathInput!.nowMillis!;
  const shift = (value: unknown): void => { if (!value || typeof value !== 'object') return; for (const [key, child] of Object.entries(value)) { if (key.endsWith('Millis') && typeof child === 'number') (value as Record<string, unknown>)[key] = child + now - old; else shift(child); } };
  shift(status.pathInput); return status;
}
// A stored ZIP keeps the development download usable without adding a compression dependency.
function mockZip(files: Record<string, string>): Buffer {
  const chunks: Buffer[] = [], directory: Buffer[] = []; let offset = 0;
  for (const [name, text] of Object.entries(files)) {
    const filename = Buffer.from(name), data = Buffer.from(text); let crc = 0xffffffff;
    for (const byte of data) { crc ^= byte; for (let bit = 0; bit < 8; bit++) crc = (crc >>> 1) ^ (crc & 1 ? 0xedb88320 : 0); }
    crc = (crc ^ 0xffffffff) >>> 0;
    const header = Buffer.alloc(30); header.writeUInt32LE(0x04034b50); header.writeUInt16LE(20, 4); header.writeUInt32LE(crc, 14); header.writeUInt32LE(data.length, 18); header.writeUInt32LE(data.length, 22); header.writeUInt16LE(filename.length, 26);
    const central = Buffer.alloc(46); central.writeUInt32LE(0x02014b50); central.writeUInt16LE(20, 4); central.writeUInt16LE(20, 6); central.writeUInt32LE(crc, 16); central.writeUInt32LE(data.length, 20); central.writeUInt32LE(data.length, 24); central.writeUInt16LE(filename.length, 28); central.writeUInt32LE(offset, 42);
    chunks.push(header, filename, data); directory.push(central, filename); offset += header.length + filename.length + data.length;
  }
  const central = Buffer.concat(directory), end = Buffer.alloc(22); end.writeUInt32LE(0x06054b50); end.writeUInt16LE(Object.keys(files).length, 8); end.writeUInt16LE(Object.keys(files).length, 10); end.writeUInt32LE(central.length, 12); end.writeUInt32LE(offset, 16);
  return Buffer.concat([...chunks, central, end]);
}

function mockQrPng(): Buffer {
  const matrix = qrMatrix('https://logs.openxplay.org/receipts/DEMO-20261027')!, size = 49;
  const chunk = (name: string, data: Buffer) => {
    const type = Buffer.from(name), all = Buffer.concat([type, data]); let crc = 0xffffffff;
    for (const byte of all) { crc ^= byte; for (let bit = 0; bit < 8; bit++) crc = (crc >>> 1) ^ (crc & 1 ? 0xedb88320 : 0); }
    const length = Buffer.alloc(4), checksum = Buffer.alloc(4); length.writeUInt32BE(data.length); checksum.writeUInt32BE((crc ^ 0xffffffff) >>> 0);
    return Buffer.concat([length, all, checksum]);
  };
  const header = Buffer.alloc(13); header.writeUInt32BE(size); header.writeUInt32BE(size, 4); header[8] = 8;
  const pixels = Buffer.alloc(size * (size + 1));
  for (let y = 0; y < size; y++) for (let x = 0; x < size; x++) pixels[y * (size + 1) + x + 1] = matrix[y - 4]?.[x - 4] ? 0 : 255;
  return Buffer.concat([Buffer.from([137,80,78,71,13,10,26,10]), chunk('IHDR', header), chunk('IDAT', deflateSync(pixels)), chunk('IEND', Buffer.alloc(0))]);
}
const MOCK_QR = mockQrPng();
function field(key: string, type: Field['type'], extra: Partial<Field> = {}): Field {
  return { key, type, labelKey: `field.${key}`, ...extra };
}
function choice(key: string, values: string[], extra: Partial<Field> = {}): Field {
  return field(key, 'choice', { options: values.map(value => ({ value, labelKey: `option.${key}.${value}` })), ...extra });
}
const when = (key: string, ...equals: Value[]) => ({ key, equals });
// Trusted HTTPS as the Linux relay describes it (public-name.json); the token never comes back.
const PUBLIC_NAME_OFF = { privacyConfigAvailable: false, publicNameEnabled: false, publicNameProvider: '', publicName: '', publicNameToken: '', publicNameTokenSet: false, publicNameAgreeTerms: false, publicNameProviders: { duckdns: { name: '', tokenSet: false }, dynv6: { name: '', tokenSet: false } } };
function publicNameSection(): Section {
  return { id: 'publicName', titleKey: 'publicName.title', fields: [
    field('publicNameEnabled', 'bool', { labelKey: 'publicName.enabled', hintKey: 'publicName.summary' }),
    choice('publicNameProvider', ['duckdns', 'dynv6'], { labelKey: 'publicName.provider' }),
    field('publicName', 'text', { labelKey: 'publicName.name', hintKey: 'publicName.nameHint' }),
    field('publicNameToken', 'secret', { labelKey: 'publicName.token', hintKey: 'publicName.tokenHint' }),
    field('publicNameAgreeTerms', 'bool', { labelKey: 'publicName.terms' }),
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
function baseSettings(linux = false): Settings {
  if (linux) return structuredClone(linuxSettings);
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
export function mockSettings(linux = false): Settings {
  if (linux) return structuredClone(linuxSettings);
  const settings = baseSettings(linux);
  const add = (id: string, additions: Field[]) => {
    const existing = new Set(settings.schema.flatMap(s => s.fields).map(f => f.key));
    settings.schema.push({ id, titleKey: `menu.${id.startsWith('developer/') ? id.replace('/', '.') : id}`, fields: additions.filter(f => !existing.has(f.key)) });
  };
  if (!linux) {
    const output = settings.schema.find(s => s.id === 'output')!;
    output.fields[0] = choice('output', ['web', 'carlink', 'wireless', 'multiple']);
    output.fields.push(choice('wirelessDisplay', ['dlna', 'miracast'], { visibleWhen: when('output', 'wireless') }));
    const display = settings.schema.find(s => s.id === 'display')!;
    display.fields[0] = choice('displayMode', ['auto', 'relay-screen', 'custom']);
    const vehicle = settings.schema.flatMap(s => s.fields).find(f => f.key === 'vehicleInformation')!; vehicle.type = 'bool';
    add('audio', [field('carBluetoothAudio', 'bool')]);
    add('output', [field('carlinkAutoReconnect', 'bool', { visibleWhen: when('output', 'carlink', 'multiple') }), field('plugins', 'bool', { hintKey: 'hint.plugins' }), field('pluginsEnabled', 'plugins')]);
    Object.assign(settings.values, { output: 'carlink', networkMode: 'phone-hotspot', wirelessDisplay: 'dlna', carBluetoothAudio: true, vehicleInformation: true, carlinkAutoReconnect: true, plugins: false, pluginsEnabled: [] });
    settings.schema.flatMap(s => s.fields).forEach(f => { if (f.visibleWhen?.key === 'output') f.visibleWhen.equals = f.visibleWhen.equals.map(v => v === 'both' ? 'multiple' : v); });
  }
  return settings;
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
  const stats = Array.from({ length: 10 }, (_, i) => ({ type: 'stats', session: '1', screen: 0,
    rendered: Math.floor((now / 1000 + i) * 59.8), dropped: Math.floor((now / 1000 + i) / 8), relayDroppedFrames: Math.floor((now / 1000 + i) / 16), decodeMs: 4, renderedLatencyMs: 30, wsBytesPerSecond: 480000 + ((now / 1000 + i) % 7) * 10000, videoFramesPerSecond: 60, decoderOutputPerSecond: 60,
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
  status.pathInput = mockPathInput(status, state);
  if ((STAGE_STATES as readonly string[]).includes(state)) { status.running = !!status.pathInput.iphone?.started; status.state = state === 'live' ? 'active' : status.running ? 'connecting' : 'idle'; }
  return status;
}
// Synthetic recorder events use only private addresses and a fictional head unit.
const traceSamples = [
  { type: 'Link', stage: 'link_joined', fields: { address: '02:00:00:00:00:01', name: 'Demo Aurora', ssid: 'Demo Aurora', interface: 'wlan0', headUnitIp: '192.168.49.1', localIp: '192.168.49.2' } },
  { type: 'Socket', service: 'RTSP', event: 'connect', remoteAddress: '192.168.49.1', port: 7236, errorClass: null },
  { type: 'Rtsp', direction: 'RX', startLine: 'OPTIONS * RTSP/1.0', headers: { CSeq: '1', 'User-Agent': 'Demo Aurora' }, body: '' },
  { type: 'Frame', direction: 'TX', port: 7237, service: 'control', category: 1, method: 4, kind: 0, sequence: 2, bodySize: 0, fields: {} },
  { type: 'Negotiation', mode: '1280x720@30', decoders: ['avc1.42E01F', 'hvc1.1.6.L120.B0'], audio: false, rtpPort: 19000, display: '1280x720' },
  { type: 'Video', event: 'format', source: 'input_session', codec: 'H264', width: 1280, height: 720, sps: ['6742001f'], pps: ['68ce3c80'] },
  { type: 'Media', liveFrames: 1800, placeholderFrames: 2, droppedFrames: 3, keyframes: 30, videoBytes: 2400000, rtpPackets: 1900, rtpBytes: 2523200, sendStalls: 1, audioFrames: 0, queueOverflows: 0 },
  { type: 'Input', kind: 'touch_down', coordinates: [[0.5, 0.5]], rawCoordinates: [[640, 360]], streamSize: [1280, 720], viewSize: [1280, 720] },
  { type: 'Error', code: 'session_failed', exceptionClass: 'ConnectionReset', location: 'rtsp' },
  { type: 'Link', stage: 'link_lost', fields: { reason: 'head unit disconnected' } },
].map((event, index) => JSON.stringify({ ...event, atNanos: 1000000000 + index * 100000000, session: 1 }));

export function mockApi(linux = false): Plugin {
  const preview = process.env.DIRELAY_PREVIEW_STATES === '1';
  let outageUntil = 0;
  let networkChanged = false;
  let stageOverride: string | null = null;
  let bluetoothRepaired = false, wifiRepaired = false, pinCancelled = false, pinIncorrect = false;
  let adminEnabled = true, setupCompleted = true, restartNeeded = false;
  let pairingStarted = 0;
  let confirmationApproved = false;
  let seededPreview = '';
  let previewInitialized = false;
  let previewPathEpoch = Date.now();
  let pairingDeadline: number | null = null;
  let pinDeadline = Date.now() / 1000 + 120;
  let pinAccepted = false;
  let sampleVehicle = false;
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
  const demoSettings = (isLinux: boolean) => {
    const result = mockSettings(isLinux);
    if (isLinux) {
      // Demo content changes values only; the schema is the production fixture verbatim.
      result.values.networkMode = 'shared-wifi';
      result.values.sharedWifiNetworks = [{ ssid: 'Home Wi-Fi', hasPassword: true, current: false }, { ssid: 'Car Wi-Fi', hasPassword: false, current: false }];
      result.installedPlugins = [{ id: 'org.openxplay.clock', name: { en: 'Clock', 'zh-CN': '时钟' }, version: '1.0.0', license: 'Apache-2.0 OR MIT', icon: null, nonCommercialAssets: false }, { id: 'org.openxplay.pet', name: { en: 'Pet', 'zh-CN': '宠物' }, version: '1.0.0', license: 'CC-BY-NC-SA-4.0', icon: null, nonCommercialAssets: true }];
    }
    return result;
  };
  let settings = demoSettings(linux);
  const logJobs = new Map<string, { job: import('./src/logs-page').LogJob; started: number }>();
  let latestReceipt: import('./src/feature-pages').LogReceipt | null = null;
  let privacyConfig: Record<string, unknown> = {};
  let identity = structuredClone(linuxFixtureStatus('idle').identity ?? { mfi: { state: 'MISSING', fingerprint: null }, tls: { state: 'MISSING', fingerprint: null, imported: false } });
  const exports = new Map<string, { data: Buffer; createdMillis: number }>();
  let traceLines = [...traceSamples];
  let capturing = false;
  const traceName = 'trace-00000001-1792695600000-00000001.jsonl';
  const mediaName = 'media-00000001-00000001-1792695600000.ts';
  const mediaBody = Buffer.alloc(188, 0xff); mediaBody.set([0x47, 0x1f, 0xff, 0x10]);
  let mediaAvailable = true;
  const publicProviders = new Map<string, { name: string; token: string }>();
  const currentSsid = () => String(settings.values.networkMode) === 'shared-wifi' ? wifiClient.ssid : null;
  const redactNetworks = () => {
    settings.values.sharedWifiNetworks = (settings.values.sharedWifiNetworks as SavedNetwork[]).map(item => ({ ssid: item.ssid, hasPassword: !!sharedPasswords.get(item.ssid), current: item.ssid === currentSsid() }));
  };
  let sharedPasswords = new Map<string, string>([["Home Wi-Fi", "openxplay"], ["Car Wi-Fi", ""]]);
  let running = false; let paused = false; let started = 0;
  let pictureTrial: Value | undefined;
  const phones = [
    { id: 'iphone-riko', name: "Riko's iPhone", kind: 'phone', connected: true, likelyIphone: true, selected: false },
    { id: 'car', name: 'Car audio', kind: 'car', connected: false, likelyIphone: false, selected: false },
    { id: 'headphones', name: 'Headphones', kind: 'other', connected: false, likelyIphone: false, selected: false },
  ];
  return { name: 'relay-mock-api', apply: 'serve', configureServer(server) {
    server.middlewares.use(async (req, res, next) => {
      const url = new URL(req.url ?? '/', 'http://localhost');
      if (preview && url.pathname === '/states') { res.setHeader('Content-Type', 'text/html; charset=utf-8'); res.end('<!doctype html><meta name="viewport" content="width=device-width, initial-scale=1"><h1>Stage previews</h1>' + ['linux', 'android'].map(platform => '<h2>' + platform + '</h2>' + STAGE_STATES.map(state => '<p><a href="/?platform=' + platform + '&state=' + state + '">' + state + '</a></p>').join('')).join('')); return; }
      if (!url.pathname.startsWith('/api/v0/')) return next();
      const path = url.pathname.slice('/api/v0'.length);
      const previewUrl = new URL(req.headers.referer ?? '/', 'http://localhost');
      const previewState = previewUrl.searchParams.get('state') ?? 'live';
      if (preview && seededPreview !== previewState) { previewInitialized = false; stageOverride = null; bluetoothRepaired = wifiRepaired = pinCancelled = pinIncorrect = confirmationApproved = false; seededPreview = previewState; }
      if (preview && (previewUrl.searchParams.get('platform') !== 'android') !== linux) { linux = previewUrl.searchParams.get('platform') !== 'android'; settings = demoSettings(linux); previewInitialized = false; }
      if (preview && !previewInitialized) {
        linux = previewUrl.searchParams.get('platform') !== 'android'; settings = demoSettings(linux);
        previewInitialized = true;
        previewPathEpoch = Date.now();
        const seed = linux ? linuxFixtureStatus(previewState, previewPathEpoch) : previewStatus({ platform: 'android' } as Status, previewState);
        running = seed.running; paused = !!seed.pathInput?.startup?.paused; started = previewPathEpoch - (running ? 10000 : 0);
        if (linux) { identity = structuredClone(seed.identity ?? identity); identity.mfi.state = seed.pathInput?.relay?.identity === 'READY' ? 'READY' : previewState === 'identity-invalid' ? 'INVALID' : 'MISSING'; }
        if (linux) { settings.values.output = seed.output; settings.values.networkMode = seed.pathInput?.setup?.network === 'SHARED_WIFI' ? 'shared-wifi' : 'device-hotspot'; }
        if (previewState === 'multiple-iphones' && !phones.some(phone => phone.id === 'iphone-second')) phones.push({ id: 'iphone-second', name: 'Second iPhone', kind: 'phone', connected: false, likelyIphone: true, selected: false });
        if (['fresh', 'pairing', 'no-iphone'].includes(previewState)) phones.splice(0);
        if (previewState === 'unpaired-iphone') settings.values.iphone = 'unpaired-phone';
        if (previewState === 'multiple-iphones') settings.values.iphone = 'auto';
        setupCompleted = previewState !== 'fresh';
        if (['carlink-pin', 'carlink-confirm', 'carlink-pin-rejected'].includes(previewState)) { pinDeadline = Date.now() / 1000 + 120; pinAccepted = false; pinIncorrect = previewState === 'carlink-pin-rejected'; }

        settings.values.carlinkTrace = 'on';
        if (linux && previewState === 'working') settings.values.networkMode = 'shared-wifi';
        if (!linux) settings.values.networkMode = previewState === 'hotspot' ? 'phone-hotspot' : 'shared-wifi';
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
      if (linux && !adminEnabled && path !== '/session' && !['127.0.0.1', '::1'].includes(req.socket.remoteAddress ?? '')) return send(403, { error: 'admin.disabled' });
      if (!['/session', '/login'].includes(path) && !ready) return send(401, { error: 'password' });
      if (method !== 'GET' && (req.headers['x-openxplay-admin'] !== '1' || !req.headers['content-type']?.startsWith('application/json'))) return send(403, { error: 'error.request' });
      let body: Record<string, unknown> = {};
      if (method !== 'GET') {
        try {
          let raw = '';
          for await (const chunk of req) { raw += chunk.toString(); if (Buffer.byteLength(raw) > (path === '/config/import' ? 65536 : linux && path === '/public-name/import' ? 16384 * 6 + 1024 : 16384)) return send(413, { error: 'error.request' }); }
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
      if (linux && path === '/identity' && method === 'GET') return send(200, identity);
      if (linux && ['/identity', '/tls'].includes(path) && method === 'DELETE') {
        if (body.confirm !== true) return send(422, { error: 'error.invalid' });
        if (path === '/identity') identity.mfi = { state: 'MISSING', fingerprint: null };
        else { identity.tls = { state: 'MISSING', fingerprint: null, imported: false }; delete privacyConfig.importedCertificate; }
        return send(200, identity);
      }
      // Crypto validation belongs to the real relay; the mock exercises the rejection UI.
      if (linux && path === '/identity/import' && method === 'POST') return body.cancel === true ? send(200, { ok: true }) : send(422, { error: 'identity_error_pair' });
      if (linux && path === '/tls/import' && method === 'POST') return send(422, { error: 'identity_error_tls' });
      if (path === '/session' && method === 'GET') return send(200, { state: ready ? 'ready' : 'login', defaultPassword, ...(linux ? { adminEnabled, adminAccessAllowed: adminEnabled || ['127.0.0.1', '::1'].includes(req.socket.remoteAddress ?? '') } : {}) });
      if (path === '/login' && method === 'POST') {
        const address = req.socket.remoteAddress ?? 'local'; const now = Date.now();
        let attempt = attempts.get(address);
        if (!attempt || now >= attempt.until) { attempt = { count: 0, until: now + 60000 }; attempts.set(address, attempt); }
        if (attempt.count >= 5) { res.setHeader('Retry-After', Math.ceil((attempt.until - now) / 1000)); return send(429, { error: 'error.rateLimit' }); }
        if (password !== body.password) { attempt.count++; return send(401, { error: 'password' }); }
        attempts.delete(address); return authenticate();
      }
      if (linux && path === '/setup/complete' && method === 'POST') { setupCompleted = true; return send(200, { ok: true }); }
      if (linux && path === '/access' && method === 'PUT') {
        if (typeof body.enabled !== 'boolean' || !body.enabled && body.confirmed !== true) return send(422, { error: 'error.invalid' });
        adminEnabled = body.enabled; return send(200, { ok: true });
      }
      if (linux && path === '/password/reset' && method === 'POST') {
        if (body.confirmed !== true) return send(422, { error: 'error.invalid' });
        password = 'openxplay'; defaultPassword = true; sessions.clear(); res.setHeader('Set-Cookie', 'direlay=; HttpOnly; SameSite=Strict; Path=/; Max-Age=0'); return send(200, { ok: true });
      }
      if (previewState === 'feature-error' && (path.startsWith('/logs') || path.startsWith('/config') || path === '/vehicle' || path.startsWith('/carlink-trace'))) return send(503, { error: 'error.request' });
      if (path === '/password' && method === 'POST') {
        if (body.current !== password) return send(403, { error: 'password' });
        if (typeof body.new !== 'string' || body.new.length < 8) return send(422, { error: 'error.passwordFormat' });
        password = body.new; defaultPassword = password === 'openxplay'; return send(200, { ok: true });
      }
      if (path === '/logs/export' && method === 'POST') {
        if (!['10m', '30m', '24h'].includes(String(body.range))) return send(422, { error: 'error.invalid' });
        for (const [id, file] of exports) if (Date.now() - file.createdMillis > 3600000) exports.delete(id);
        while (exports.size >= 2) exports.delete(exports.keys().next().value!);
        const id = randomUUID(), createdMillis = Date.now();
        const data = mockZip({ 'relay.log': `OpenXPlay mock log, range ${body.range}\nBluetooth <redacted>\n`, 'info.txt': `platform=${linux ? 'linux' : 'android'}\nversion=0.2.1\n`, 'timeline.txt': 'Carlink scanning → Wi-Fi joined → first video received\n' });
        exports.set(id, { data, createdMillis });
        return send(200, { id, bytes: data.length, sha256: createHash('sha256').update(data).digest('hex'), createdMillis, firstMillis: createdMillis - 60000, lastMillis: createdMillis, empty: previewState === 'feature-empty', truncated: false });
      }
      if (path.startsWith('/logs/export/') && method === 'GET') {
        const file = exports.get(decodeURIComponent(path.slice('/logs/export/'.length)));
        if (!file || Date.now() - file.createdMillis > 3600000) return send(404, { error: 'error.request' });
        res.setHeader('Content-Type', 'application/zip'); res.setHeader('Content-Disposition', `attachment; filename="openxplay-${linux ? 'linux' : 'android'}-logs-${file.createdMillis}.zip"`); return res.end(file.data);
      }
      if (path === '/logs/upload' && method === 'POST') {
        const error = previewUrl.searchParams.get('uploadError'); if (error && ['offline', 'too_large', 'rejected', 'timeout'].includes(error)) return send(503, { error });
        if (!exports.has(String(body.id))) return send(404, { error: 'error.request' });
        const receipt = { logId: 'DEMO-20261027', trackingUrl: 'https://logs.openxplay.org/receipts/DEMO-20261027', qrCodeUrl: '/api/v0/logs/qr.png', expiresMillis: Date.now() + 48 * 3600000 };
        if (!linux) { latestReceipt = receipt; return send(200, receipt); }
        if ([...logJobs.values()].some(entry => entry.job.state === 'running')) return send(409, { error: 'rejected' });
        const id = randomUUID(), total = exports.get(String(body.id))!.data.length;
        const job: import('./src/logs-page').LogJob = { id, exportId: String(body.id), state: 'running', phase: 'Connecting', sent: 0, total, elapsedMillis: 0, receipt: null, error: null };
        if (logJobs.size >= 8) logJobs.delete(logJobs.keys().next().value!);
        logJobs.set(id, { job, started: Date.now() }); return send(200, job);
      }
      if (linux && path.startsWith('/logs/upload/') && ['GET', 'DELETE'].includes(method)) {
        const entry = logJobs.get(decodeURIComponent(path.slice('/logs/upload/'.length)));
        if (!entry) return send(404, { error: 'error.request' });
        const job = entry.job;
        if (job.state === 'running') {
          job.elapsedMillis = Date.now() - entry.started;
          if (method === 'DELETE') job.state = 'cancelled';
          else if (job.elapsedMillis >= 3000) {
            job.state = 'completed';
            latestReceipt = job.receipt = { logId: 'DEMO-20261027', trackingUrl: 'https://logs.openxplay.org/receipts/DEMO-20261027', qrCodeUrl: '/api/v0/logs/qr.png', expiresMillis: Date.now() + 48 * 3600000 };
          } else if (job.elapsedMillis >= 2000) { job.phase = 'AwaitingReceipt'; job.sent = job.total; }
          else if (job.elapsedMillis >= 1000) { job.phase = 'Sending'; job.sent = Math.floor(job.total / 2); }
        }
        return send(200, job);
      }
      if (path === '/logs/receipt' && method === 'GET') return send(latestReceipt ? 200 : 404, latestReceipt ?? { error: 'error.request' });
      if (path === '/logs/qr.png' && method === 'GET') { res.setHeader('Content-Type', 'image/png'); return res.end(MOCK_QR); }
      if (['/config/user', '/config/privacy'].includes(path) && method === 'GET') {
        const privacy = path.endsWith('privacy');
        res.setHeader('Content-Disposition', `attachment; filename="openxplay-${privacy ? 'privacy' : 'user'}-config.json"`);
        const safe = Object.fromEntries(Object.entries(settings.values).filter(([key]) => !/password|token|publicNameProviders|carlinkPictureTry/i.test(key)));
        return send(200, { format: privacy ? 'openxplay-privacy-config' : 'openxplay-user-config', version: 1, platform: linux ? 'linux' : 'android', appVersion: '0.2.1', exportedAt: new Date().toISOString(), ...(privacy ? privacyConfig : { settings: safe }) });
      }
      if (path === '/config/import' && method === 'POST') {
        if (!['openxplay-user-config', 'openxplay-privacy-config'].includes(String(body.format)) || body.version !== 1) return send(422, { error: 'error.invalid' });
        if (body.format === 'openxplay-privacy-config') {
          const keys = Object.keys(body).filter(key => !['format', 'version', 'platform', 'appVersion', 'exportedAt'].includes(key));
          const allowed = ['publicName', 'publicNameCertificate', 'publicNameAccount', 'providers', 'mfiRemoteToken'];
          if ('mfiRemoteToken' in body && (typeof body.mfiRemoteToken !== 'string' || /[\0\r\n]/.test(body.mfiRemoteToken))) return send(422, { errors: { mfiRemoteToken: 'invalid' } });
          const applied = keys.filter(key => allowed.includes(key)), ignored = keys.filter(key => !allowed.includes(key));
          if (url.searchParams.get('dryRun') !== '1') privacyConfig = { ...privacyConfig, ...Object.fromEntries(applied.map(key => [key, body[key]])) };
          return send(200, { applied, ignored, restartNeeded: applied.includes('mfiRemoteToken'), appliedNow: applied.filter(key => key !== 'mfiRemoteToken'), nextStart: applied.filter(key => key === 'mfiRemoteToken') });
        }
        if (!body.settings || typeof body.settings !== 'object' || Array.isArray(body.settings)) return send(422, { error: 'error.invalid' });
        const known = new Set(settings.schema.flatMap(s => s.fields).map(f => f.key).filter(key => key !== 'carlinkPictureTry'));
        const changes = Object.entries(body.settings as Record<string, Value>), applied = changes.filter(([key]) => known.has(key)).map(([key]) => key), ignored = changes.filter(([key]) => !known.has(key)).map(([key]) => key);
        const nextStart = applied.filter(key => ['hotspotChannel', 'hotspotBand', 'carBluetoothAudio', 'carlinkPictureDefault', 'carlinkPictureSize', 'mfiTarget', 'mfiI2cPath', 'mfiRemoteServer'].includes(key));
        const restartNeeded = nextStart.length > 0;
        if (url.searchParams.get('dryRun') !== '1') for (const [key, value] of changes) if (known.has(key)) settings.values[key] = value;
        return send(200, { applied, ignored, restartNeeded, appliedNow: applied.filter(key => !nextStart.includes(key)), nextStart });
      }
      if (path === '/update' && method === 'GET') {
        if (!['stable', 'beta'].includes(url.searchParams.get('channel') ?? '')) return send(422, { error: 'error.invalid' });
        if (previewUrl.searchParams.has('offline')) return send(503, { error: 'offline' });
        const channel = url.searchParams.get('channel') as 'stable' | 'beta';
        const candidates = {
          stable: { latest: '0.2.1', outcome: 'up-to-date', url: 'https://github.com/OpenXPlay/OpenXPlay-Relay/releases', notes: '', publishedAt: '2026-10-25T12:00:00Z' },
          beta: { latest: '0.2.2-beta.1', outcome: 'available', url: 'https://github.com/OpenXPlay/OpenXPlay-Relay/releases', notes: 'Connection improvements · 连接改进\n\n**Changes**\n- Carlink connection improvements.\n\n**更新**\n- Carlink 连接改进。', publishedAt: '2026-10-27T12:00:00Z' },
        };
        return send(200, { current: '0.2.1', ...candidates[channel], channel, candidates, checkedMillis: Date.now() });
      }
      if (linux && ['/network-roles', '/network-roles/preview'].includes(path)) {
        const network = mockNetwork(String(settings.values.networkMode));
        if (!body.networkRoles || typeof body.networkRoles !== 'object') return send(422, { error: 'error.invalid' });
        const roles = body.networkRoles as import('./src/schema').NetworkRolesSetting;
        if (path === '/network-roles') { settings.values.networkRoles = roles; if (body.wifiCountry) settings.values.wifiCountry = String(body.wifiCountry); }
        return send(200, { plan: { ...network.plan, source: roles.mode }, checks: [], changes: ['controller', 'carlink'], ...(path === '/network-roles' ? { applied: true } : {}) });
      }
      if (linux && path === '/vehicle' && method === 'GET') return send(200, { sample: sampleVehicle, enabled: sampleVehicle || settings.values.vehicleInformation === true, sensorActive: false, malformedFrames: 0, storageFailed: false, reports: sampleVehicle ? [{ kind: 'Gear', fields: { gear: 'D', speed: 40 }, receivedAt: Date.now() }] : [], history: [] });
      if (linux && path === '/vehicle/sample' && method === 'POST') { if (typeof body.enabled !== 'boolean') return send(422, { error: 'error.invalid' }); sampleVehicle = body.enabled; return send(200, { ok: true }); }
      if (linux && path === '/actions/clear-vehicle-history' && method === 'POST') return send(200, { ok: true });
      if (linux && path === '/carlink/pairing' && method === 'POST') {
        const confirmation = previewState === 'carlink-confirm' && !confirmationApproved; const expected = confirmation ? 18 : 17;
        if (!['approve', 'pin', 'cancel'].includes(String(body.action)) || body.request !== expected || pinAccepted || pinCancelled || !['carlink-pin', 'carlink-confirm', 'carlink-pin-rejected'].includes(previewState)) return send(409, { error: 'error.carlinkPinNotRequested' });
        if (body.action === 'cancel') { pinCancelled = true; return send(200, { ok: true }); }
        if (body.action === 'approve') { if (!confirmation) return send(422, { error: 'error.carlinkPin' }); confirmationApproved = true; return send(200, { ok: true, request: 17 }); }
        if (!/^\d{1,128}$/.test(String(body.pin))) return send(422, { error: 'error.carlinkPin' });
        if (body.pin === '0000') { pinIncorrect = true; return send(200, { ok: true }); }
        pinIncorrect = false; pinAccepted = true; running = true; stageOverride = 'starting'; started = Date.now(); return send(200, { ok: true });
      }
      if (path === '/carlink/pin' && method === 'POST') { if (!/^\d{4,8}$/.test(String(body.pin))) return send(422, { error: 'error.carlinkPin' }); pinAccepted = true; return send(200, { ok: true }); }
      if (path === '/carlink/connect' && method === 'POST') return send(200, { ok: true });
      if (path === '/actions/clear-saved-head-unit' || path === '/actions/forget-carlink') return send(200, { ok: true });
      if (linux && (path === '/carlink-trace' || path.startsWith('/carlink-trace/'))) {
        const enabled = settings.values.carlinkTrace === 'on';
        const traceRunning = enabled && (preview || running);
        const traceBody = traceLines.length ? traceLines.join('\n') + '\n' : '';
        const files = previewState === 'feature-empty' ? [] : [
          ...(traceLines.length ? [{ name: traceName, kind: 'trace', size: Buffer.byteLength(traceBody), modified: 1792695600 }] : []),
          ...(mediaAvailable ? [{ name: mediaName, kind: 'media', size: mediaBody.length, modified: 1792695600 }] : []),
        ];
        if (path === '/carlink-trace' && method === 'GET') return send(200, { enabled, running: traceRunning,
          status: { type: 'clink-trace', lines: traceLines.length, bytes: Buffer.byteLength(traceBody), dropped: 3, failure: null,
            capture: { state: capturing ? 'capturing' : 'idle', bytes: capturing ? 1880 : 188, file: capturing ? mediaName + '.part' : null, reason: null } }, files });
        if (path === '/carlink-trace/tail' && method === 'GET') {
          const type = url.searchParams.get('type');
          const limit = Number(url.searchParams.get('limit') ?? 5000);
          if (!Number.isInteger(limit) || limit < 0 || limit > 5000 || (type !== null && !['Rtsp', 'Frame', 'Socket', 'Negotiation', 'Media', 'Video', 'Input', 'Error', 'Link'].includes(type))) return send(422, { error: 'error.invalid' });
          return send(200, { lines: (previewState === 'feature-empty' ? [] : traceLines).filter(line => !type || JSON.parse(line).type === type).slice().reverse().slice(0, limit) });
        }
        if (method === 'GET' && (path === '/carlink-trace/download' || path.startsWith('/carlink-trace/files/'))) {
          const bundle = path === '/carlink-trace/download';
          const name = path.slice('/carlink-trace/files/'.length);
          if (!bundle && !files.some(file => file.name === name)) return send(404, { error: 'error.request' });
          const media = !bundle && name === mediaName;
          const data = media ? mediaBody : Buffer.from(traceBody);
          res.statusCode = 200;
          res.setHeader('Content-Type', media ? 'video/mp2t' : 'application/x-ndjson');
          res.setHeader('Content-Disposition', `attachment; filename="${bundle ? 'carlink-trace-demo.jsonl' : name}"`);
          res.setHeader('Content-Length', data.length);
          res.setHeader('Cache-Control', 'no-store');
          return res.end(data);
        }
        if (path === '/carlink-trace/clear' && method === 'POST') {
          capturing = false; traceLines = []; mediaAvailable = false;
          return send(200, { ok: true });
        }
        if (path === '/carlink-trace/capture' && method === 'POST') {
          if (!['start', 'stop'].includes(String(body.action))) return send(422, { error: 'error.invalid' });
          if (!traceRunning) return send(409, { error: 'error.carlinkTraceOff' });
          capturing = body.action === 'start';
          if (!capturing) mediaAvailable = true;
          return send(200, { ok: true });
        }
        return send(404, { error: 'error.request' });
      }
      if (linux && path === '/hotspot' && method === 'PUT') {
        if (typeof body.ssid !== 'string' || typeof body.password !== 'string') return send(422, { error: 'error.invalid' });
        if (body.band !== undefined && !['auto', '2.4', '5'].includes(body.band)) return send(422, { error: 'error.invalid' });
        const error = validateHotspot(body.ssid, body.password, true);
        if (error) return send(422, { error });
        if (body.security !== undefined && !['open', 'wpa2', 'wpa3-transition', 'wpa3'].includes(body.security)) return send(422, { error: 'error.invalid' });
        settings.values.hotspotSsid = body.ssid; if (body.password) settings.values.hotspotPassword = body.password;
        if (body.security !== undefined) settings.values.hotspotSecurity = body.security;
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
      if (path === '/settings' && method === 'GET') return send(200, linux ? { ...settings, values: { ...settings.values, privacyConfigAvailable: previewState !== 'feature-empty' && !!settings.values.publicNameProvider || Object.entries(privacyConfig).some(([, value]) => !!value), publicNamePrivacyAvailable: !!settings.values.publicNameProvider || Object.keys(privacyConfig).some(key => key.startsWith('publicName') || key === 'providers'), mfiRemoteTokenSet: !!privacyConfig.mfiRemoteToken, mfiSourceStatus: settings.values.mfiTarget === 'remote' ? 'ready' : settings.values.mfiSourceStatus, carlinkPictureTry: pictureTrial === undefined ? settings.values.carlinkPictureDefault : pictureTrial } } : settings);
      if (path === '/phones' && method === 'GET') { if (pairingStarted && Date.now() - pairingStarted >= 2000 && !phones.some(phone => phone.id === 'iphone-new')) phones.push({ id: 'iphone-new', name: 'New iPhone', kind: 'phone', connected: true, likelyIphone: true, selected: false }); return send(200, phones.map(phone => ({ ...phone, selected: settings.values.iphone === phone.id }))); }
      if (path === '/settings' && method === 'PUT') {
        const errors: Record<string, string> = {};
        const mfi = linux ? mfiError({ ...settings.values, ...body }) : null;
        if (mfi) errors.mfiTarget = mfi;
        if ('mfiRemoteToken' in body && (typeof body.mfiRemoteToken !== 'string' || /[\0\r\n]/.test(body.mfiRemoteToken))) errors.mfiRemoteToken = 'error.invalid';
        for (const [key, value] of Object.entries(body)) {
          const field = settings.schema.flatMap(section => section.fields).find(field => field.key === key);
          if (!field) { errors[key] = 'error.invalid'; continue; }
          if (key === 'experimentalBluetoothAddress' && (body.experimentalActive ?? settings.values.experimentalActive) !== false && (body.experimentalSendBluetoothAddress ?? settings.values.experimentalSendBluetoothAddress) !== false && value !== '' && (typeof value !== 'string' || !/^[0-9a-f]{2}(:[0-9a-f]{2}){5}$/i.test(value) || (parseInt(value.slice(0, 2), 16) & 1) !== 0 || ['00:00:00:00:00:00', '02:00:00:00:00:00'].includes(value.toLowerCase()))) errors[key] = 'error.invalid';
          const pictureSize = (size: unknown) => !!size && typeof size === 'object' && validPictureSize(size as import('./src/schema').PictureSize);
          if (key === 'carlinkPictureSizes' && (!Array.isArray(value) || value.length > 5 || !value.every(pictureSize) || normalizedSizes(value as import('./src/schema').PictureSize[]).length !== value.length)) errors[key] = 'error.invalid';
          if (['carlinkPictureDefault', 'carlinkPictureTry'].includes(key) && value !== null && !pictureSize(value)) errors[key] = 'error.invalid';
          if (field.type === 'object') {
            if (value !== null && typeof value !== 'object') errors[key] = 'error.invalid';
            continue;
          } else if (field.type === 'plugins') {
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
        if (linux && 'carlinkPictureTry' in body && settings.values.output === 'web') errors.carlinkPictureTry = 'error.conflict';
        if (linux && ('carlinkPictureDefault' in body || 'carlinkPictureSizes' in body)) {
          const nextDefault = Object.hasOwn(body, 'carlinkPictureDefault') ? body.carlinkPictureDefault : settings.values.carlinkPictureDefault;
          const saved = body.carlinkPictureSizes ?? settings.values.carlinkPictureSizes;
          const choices = [...picturePresets, ...(Array.isArray(saved) ? saved : []) as import('./src/schema').PictureSize[]];
          const selected = nextDefault as import('./src/schema').PictureSize | null;
          if (selected && !choices.some(size => size.width === selected.width && size.height === selected.height)) errors.carlinkPictureDefault = 'error.invalid';
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
        if ('carlinkPictureTry' in body) { pictureTrial = body.carlinkPictureTry as Value; delete body.carlinkPictureTry; started = Date.now(); }
        const mfiChanged = Object.keys(body).some(key => key.startsWith('mfi'));
        if (typeof body.mfiRemoteToken === 'string') { privacyConfig.mfiRemoteToken = body.mfiRemoteToken; delete body.mfiRemoteToken; }
        if (running && Object.keys(body).some(key => ['iphone', 'resolution', 'screenWidth', 'videoCodec', 'frameRate', 'carBluetoothAudio', 'displayMode', 'displayWidth', 'displayHeight', 'displayDensity'].includes(key))) restartNeeded = true;
        Object.assign(settings.values, body);
        if (body.networkMode !== undefined) {
          networkChanged = true; outageUntil = Date.now() + 2000;
          const shared = body.networkMode === 'shared-wifi';
          wifiClient = shared ? { ssid: (settings.values.sharedWifiNetworks as SavedNetwork[])[0].ssid, ip: '192.168.1.78', signal: 82, setupHotspot: false } : { ssid: null, ip: null, signal: null, setupHotspot: false };
          redactNetworks();
        }
        return send(200, { ok: true, applied: mfiChanged ? 'next-start' : body.networkMode !== undefined || body.sharedWifiNetworks !== undefined || (linux && Object.keys(body).every(key => key.startsWith('publicName') || key.startsWith('experimental') || ['autoStart', 'bootStart', 'plugins', 'pluginsEnabled', 'output', 'requirePairingCode', 'carlinkLink', 'carlinkAddress', 'carlinkHeadUnit', 'carlinkLocal'].includes(key))) ? 'now' : running ? 'next-start' : 'now' });
      }
      if (path === '/status' && method === 'GET') {
        if (Date.now() < outageUntil) return send(503, { error: 'error.network' });
        const phone = phones.find(phone => phone.id === settings.values.iphone) ?? phones[0];
        const status: Status = {
          ...(linux ? linuxFixtureStatus(preview ? previewState : running ? 'live-browser' : 'idle') : {}),
          platform: linux ? 'linux' : 'android', name: 'OpenXPlay Relay', version: '0.2.1', host: `openxplay-${linux ? 'linux' : 'android'}.local`, adminUrl: `https://openxplay-${linux ? 'linux' : 'android'}.local:6975/`,
          wifiControl: linux,
          hotspot: { security: (settings.values.hotspotSecurity ?? 'wpa2') as 'open' | 'wpa2' | 'wpa3-transition' | 'wpa3', supportedSecurity: ['open', 'wpa2', 'wpa3-transition', 'wpa3'], hasPassword: settings.values.hotspotSecurity !== 'open', ssid: String(settings.values.hotspotSsid ?? 'OpenXPlay'), password: String(settings.values.hotspotPassword ?? 'openxplay123'), ip: settings.values.networkMode !== 'shared-wifi' ? '10.42.0.1' : wifiClient.setupHotspot ? '10.78.0.1' : null, active: settings.values.networkMode !== 'shared-wifi' || wifiClient.setupHotspot === true },
          ...(linux ? { wifiProvisioning: true, wifiClient } : {}),
          receiverPort: 8443, adminPort: 6975,
          hostAddresses: [{ interface: 'wlan0', ipv4: settings.values.networkMode !== 'shared-wifi' ? '10.42.0.1' : wifiClient.ip ?? '192.168.5.171', developerOnly: false }],
          receiverUrls: linux ? ['https://openxplay-linux.local:8443/'] : ['https://192.168.5.171:8443/', 'https://openxplay-android.local:8443/'],
          pairingAvailable: true, pairingCancelable: linux, pairedDevices: phones.map(phone => ({ name: phone.name, connected: phone.connected, iphone: phone.likelyIphone })),
          video: running ? { codec: 'H265', reason: 'all-main-screen-outputs-hardware-hevc', codecString: 'hvc1.1.6.L120.B0', width: 1920, height: 720 } : { codec: 'H264', reason: 'no-main-screen-output' },
          state: !running ? 'idle' : Date.now() - started < 3000 ? 'connecting' : 'active', running, autoStartPaused: paused,
          iphone: { name: phone?.name ?? null, reason: !phone ? 'none' : running ? 'connected' : settings.values.iphone === 'auto' ? 'onlyCandidate' : 'selected' },
          network: { mode: String(settings.values.networkMode), wifi: currentSsid() },
          output: String(settings.values.output), carlink: { state: running ? 'connected' : 'idle', detail: '' }, notices,
          ...(linux ? { publicName: mockPublicName(settings.values), wireless: mockWireless(settings.values) } : {}),
        };
        if (status.publicName?.state === 'ready') status.receiverUrls = [`https://${status.publicName.name}:8443/`, ...status.receiverUrls];
        if (linux) status.adminUrls = [...(status.publicName?.trustedName ? [`https://${status.publicName.trustedName}:6975/`] : []), `https://${status.hostAddresses![0].ipv4}:6975/`, status.adminUrl];
        const now = Date.now();
        let state = stageOverride ?? (preview ? previewState : running ? 'starting' : 'idle');
        if (state === 'arrival' && !paused && settings.values.autoStart === true && now >= previewPathEpoch + 5000) { running = true; stageOverride = state = 'starting'; started = now; }
        if (state === 'starting') state = now - started < 1000 ? 'startup-bluetooth' : now - started < 2000 ? 'startup-auth' : now - started < 3000 ? 'startup-wifi' : settings.values.output === 'carlink' ? 'live-carlink' : 'live-browser';
        const result = linux ? status : preview ? previewStatus(status, state) : status;
        const seed = linux ? linuxFixtureStatus(state, previewPathEpoch) : result;
        const input = normalizeInput(linux ? seed.pathInput : mockPathInput(result, state, now, previewPathEpoch));
        input.nowMillis = now;
        input.setup.network = settings.values.networkMode === 'shared-wifi' ? 'SHARED_WIFI' : 'PHONE_HOTSPOT';
        input.setup.outputs = settings.values.output === 'web' ? ['BROWSER'] : settings.values.output === 'carlink' ? ['CARLINK'] : ['BROWSER', 'CARLINK'];
        if (input.setup.outputs.includes('BROWSER')) { if (input.car.web.phase === 'OFF') input.car.web.phase = 'LISTENING'; input.car.pageUrl = result.receiverUrls[0] ?? null; }
        else { input.car.web.phase = 'OFF'; input.car.pageUrl = null; input.car.pages = 0; }
        if (!input.setup.outputs.includes('CARLINK')) input.car.carlink.phase = 'OFF';
        else if (input.car.carlink.phase === 'OFF') input.car.carlink.phase = running ? 'PLAYING' : 'SCANNING';
        const chosen = phones.find(phone => phone.id === settings.values.iphone);
        const candidates = phones.filter(phone => phone.likelyIphone);
        input.iphone.pick = settings.values.iphone !== 'auto' ? chosen ? 'PICKED' : 'UNPAIRED' : candidates.length > 1 ? 'MULTIPLE' : candidates.length ? 'PICKED' : 'NONE';
        if (settings.values.autoStart !== true || paused) input.iphone.armed = false;
        input.iphone.name = chosen?.name ?? (input.iphone.pick === 'PICKED' ? candidates[0]?.name : null) ?? null;
        if (!preview || stageOverride) { input.iphone.started = running; input.iphone.armed = !running && !paused && settings.values.autoStart === true; input.iphone.failure = null; }
        input.relay.bluetoothOn = bluetoothRepaired || input.relay.bluetoothOn;
        input.relay.wifiOn = wifiRepaired || input.relay.wifiOn;
        input.relay.sharedWifiSsid = result.network.wifi; input.relay.wifiSsid = result.network.wifi;
        input.relay.hotspotOn = !!result.hotspot?.active; input.relay.hotspotSsid = result.hotspot?.ssid ?? null;
        input.car.address = result.hostAddresses?.[0]?.ipv4 ?? null;
        input.startup = { automatic: settings.values.autoStart === true && !paused, bootStart: settings.values.bootStart === true, paused, arrival: { pending: state === 'arrival' && !paused && settings.values.autoStart === true, deadlineMillis: state === 'arrival' && !paused ? previewPathEpoch + 5000 : null, remainingMillis: state === 'arrival' && !paused ? Math.max(0, previewPathEpoch + 5000 - now) : null } };
        result.pathInput = input;
        result.running = running; result.state = input.live.pictureOnCar ? 'active' : running ? 'connecting' : 'idle';
        result.autoStartPaused = paused; result.iphone = { name: input.iphone.name, reason: ({ PICKED: 'Selected', NONE: 'NoIphone', UNPAIRED: 'SelectedUnpaired', MULTIPLE: 'MultipleCandidates', NOT_IPHONE: 'OnlyBonded' })[input.iphone.pick] };
        result.output = String(settings.values.output); result.notices = restartNeeded ? ['notice.nextStart'] : notices;
        if (linux) { result.adminEnabled = adminEnabled; result.setupCompleted = setupCompleted; result.adminFeatures = ['logs', 'config-files', 'update-check']; result.screenData = seed.screenData; }
        if (linux) { const facts = mockNetwork(result.network.mode); const roles = settings.values.networkRoles as import('./src/schema').NetworkRolesSetting; if (roles?.mode === 'custom') { facts.plan!.source = 'custom'; if (roles.iphone) facts.plan!.iphone.link = roles.iphone.link; if (roles.adminHotspot) Object.assign(facts.plan!.adminHotspot, roles.adminHotspot); } if (facts.country) facts.country.configured = String(settings.values.wifiCountry || 'system'); result.network = { ...facts, ...result.network }; result.adminHotspot = { state: roles?.adminHotspot?.enabled ? 'up' : 'off', reason: null }; }
        if (linux) { result.adminFeatures = ['logs', 'config-files', 'update-check', 'identity']; result.identity = structuredClone(identity); input.relay.identity = settings.values.mfiTarget !== 'local' ? 'READY' : identity.mfi.state === 'READY' ? 'READY' : 'MISSING'; input.relay.identityLabel = identity.mfi.state; }
        if (state === 'pairing' && !pairingStarted) { pairingStarted = now; pairingDeadline = now / 1000 + 600; }
        result.pairingDeadline = pairingDeadline && pairingDeadline > now / 1000 ? pairingDeadline : null;
        result.pairingRemaining = result.pairingDeadline ? Math.ceil(result.pairingDeadline - now / 1000) : null;
        if (linux && result.carlink) {
          result.carlink = { ...result.carlink, ...seed.carlink, vehicle: { enabled: settings.values.vehicleInformation === true, state: settings.values.vehicleInformation === true ? 'waiting' : 'off', sensorActive: false, storageFailed: false } };
          if (['carlink-pin', 'carlink-confirm', 'carlink-pin-rejected'].includes(previewState)) {
            Object.assign(result.carlink, { pinRequested: !pinAccepted && !pinCancelled, pinIncorrect, pinRequest: pinAccepted || pinCancelled ? null : previewState === 'carlink-confirm' && !confirmationApproved ? 18 : 17, pinConfirmation: previewState === 'carlink-confirm' && !confirmationApproved, pinDeadline, trusted: pinAccepted });
            if (pinCancelled) { input.car.carlink.phase = 'ENDED'; input.car.carlink.pinPending = false; }
            if (pinAccepted) { input.car.carlink.phase = 'PLAYING'; input.car.carlink.pinPending = false; input.car.carlink.trusted = true; }
          }
          if (pictureTrial && typeof pictureTrial === 'object' && 'width' in pictureTrial && 'height' in pictureTrial && input.car.carlink.phase === 'PLAYING') { result.carlink.appliedPicture = { width: Number(pictureTrial.width), height: Number(pictureTrial.height) }; input.car.carlink.picture = `${pictureTrial.width}×${pictureTrial.height} H.265`; }
        }
        if (linux && result.video) { const seconds = now / 1000, active = result.state === 'active'; Object.assign(result.video, { session: active ? '1' : null, active, frames: Math.floor(seconds * 60), bytes: Math.floor(seconds * 480000), bytesMeasured: settings.values.videoStats !== 'off', appliedFps: 60, requestedFps: 60, hevcFailure: null }); }
        if (url.searchParams.get('diagnostics') === '1') result.receivers = running || preview ? mockReceivers() : [];
        return send(200, result);
      }
      if (preview && path === '/carlink/connect' && method === 'POST') return send(200, { ok: true });
      if (path === '/actions/pair-iphone' && method === 'POST') { pairingStarted ||= Date.now(); if (!pairingDeadline || pairingDeadline <= Date.now() / 1000) pairingDeadline = Date.now() / 1000 + 600; return send(200, { ok: true }); }
      if (linux && path === '/actions/cancel-pairing' && method === 'POST') { pairingDeadline = null; return send(200, { ok: true }); }
      if (linux && path === '/actions/public-name-renew' && method === 'POST') return send(200, { ok: true });
      if (linux && path === '/actions/public-name-remove' && method === 'POST') { publicProviders.clear(); Object.assign(settings.values, PUBLIC_NAME_OFF); return send(200, { ok: true }); }
      if (path.startsWith('/actions/') && method === 'POST') {
        const name = path.split('/').pop();
        if (!['start', 'stop', 'restart', 'cancel-arrival', 'enable-bluetooth', 'enable-wifi', 'regenerate-pairing-code'].includes(name!)) return send(404, { error: 'error.request' });
        if (name === 'enable-bluetooth') bluetoothRepaired = true;
        else if (name === 'enable-wifi') wifiRepaired = true;
        else if (name !== 'regenerate-pairing-code') { running = !['stop', 'cancel-arrival'].includes(name!); paused = !running; started = Date.now(); stageOverride = running ? 'starting' : 'ready'; restartNeeded = false; if (running) setupCompleted = true; }
        return send(200, { ok: true });
      }
      send(404, { error: 'error.request' });
    });
  } };
}
