// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-15

import type { Session, WifiNetwork, Status, CarlinkStatus } from './api';
import type { Settings, SavedNetwork } from './schema';
export function sessionRoute(session: Session): 'login' | 'ready' { return session.state; }
export function wifiGuidance(action: 'join' | 'forget'): string {
  return action === 'join' ? 'wifi.joinGuidance' : 'wifi.forgetGuidance';
}
export function sortNetworks(networks: WifiNetwork[]): WifiNetwork[] {
  return [...networks].sort((a, b) => Number(b.known) - Number(a.known) || b.signal - a.signal || a.ssid.localeCompare(b.ssid));
}

export function wifiJoinCredentials(selected: WifiNetwork | null, manual: boolean, ssid: string, password: string): { ssid: string; password: string } | null {
  const name = manual ? ssid : selected?.ssid;
  if (!name || !name.trim() || new TextEncoder().encode(name).length > 32 || name.includes('\0') || password.includes('\0')) return null;
  return { ssid: name, password: manual || selected?.secure ? password : '' };
}

export function carlinkView(status: Pick<Status, 'output' | 'carlink'>, now = Date.now() / 1000) {
  const carlink = status.carlink;
  const seconds = Math.max(0, Math.ceil((carlink?.pinDeadline ?? 0) - now));
  const visible = ['carlink', 'both'].includes(status.output);
  return { visible, seconds, pinRequested: visible && !!carlink?.pinRequested && seconds > 0 };
}
export function carlinkPinError(carlink: CarlinkStatus | null | undefined): string {
  return carlink?.pinIncorrect ? 'carlink.pinIncorrect' : '';
}
export function validCarlinkPin(pin: string): boolean { return /^[0-9]{4,8}$/.test(pin); }

export function carlinkDetail(carlink: CarlinkStatus | null): string {
  const reason = carlink?.lastLostReason || carlink?.detail || '';
  const [code, frequency] = reason.split(':');
  if (code === 'head_unit_band_unsupported') {
    const freq = Number(frequency);
    return frequency === undefined || (freq >= 5000 && freq < 6000)
      ? 'carlink.bandUnsupported5ghz' : 'carlink.bandUnsupported';
  }
  return carlink?.detail || reason;
}

export function canConnectCarlink(pin: string, carlink: CarlinkStatus | null): boolean {
  return validCarlinkPin(pin) || (pin === '' && carlink?.trusted === true);
}


export function iphoneReason(reason: string): string {
  return ({ NoIphone: 'none', SelectedUnpaired: 'unpaired', Connected: 'connected', Selected: 'selected', OnlyCandidate: 'onlyCandidate', OnlyBonded: 'onlyCandidate', MultipleCandidates: 'multiple', MultipleConnected: 'multiple' } as Record<string, string>)[reason] ?? reason;
}
export function homeView(status: Status, now = Date.now() / 1000, defaultPassword = false) {
  const noNetwork = status.wifiClient?.setupHotspot === true || (status.network.mode === 'shared-wifi' && !status.network.wifi);
  const noPhone = ['none', 'unpaired'].includes(iphoneReason(status.iphone.reason));
  const action = noNetwork ? 'wifi' : noPhone ? (status.pairingAvailable ? 'pair' : null)
    : ['carlink', 'both'].includes(status.output) && (status.carlink?.trusted !== true || (status.platform === 'linux' && status.carlink?.state === 'paused')) ? 'car' : defaultPassword ? 'password' : null;
  const active = ['active', 'Active', 'Streaming', 'WirelessActive'].includes(status.state);
  // The Bluetooth guard is only news while it recovers or after it gave up.
  const bluetooth = status.bluetoothGuard?.state === 'recovering' || status.bluetoothGuard?.state === 'failed' ? status.bluetoothGuard.state : null;
  return { action, bluetooth, seconds: Math.max(0, Math.ceil((status.pairingDeadline ?? 0) - now)),
    hero: noNetwork ? 'home.network' : noPhone ? 'home.phone' : action === 'car' ? 'home.car' : active && status.screenConnected ? 'home.screenActive' : active ? 'home.active' : 'home.waiting',
    tone: status.notices.length || noNetwork || noPhone || bluetooth ? 'attention' : active ? 'ok' : 'attention',
    promoteAddresses: action === null && status.output !== 'carlink' && status.screenConnected !== true,
    phoneConnected: iphoneReason(status.iphone.reason) === 'connected',
    carConnected: ['carlink', 'both'].includes(status.output) ? status.carlink?.trusted === true && status.carlink.state === 'connected' : status.screenConnected === true,
    carRows: [status.carlink?.headUnit?.name ? { label: 'carlink.headUnit', value: status.carlink.headUnit.name } : null, status.carlink?.network?.ssid ? { label: 'ui.network', value: status.carlink.network.ssid } : null].filter(row => row !== null), active };
}
export function addressViews(status: Status, developer: boolean) {
  const addresses = status.hostAddresses ?? status.receiverUrls.flatMap(url => {
    try { const host = new URL(url).hostname; return /^\d+\.\d+\.\d+\.\d+$/.test(host) ? [{ interface: '', ipv4: host, developerOnly: false }] : []; } catch { return []; }
  });
  // Trusted HTTPS: while ready, the name replaces the published address; the IP stays as fallback.
  const named = publicNameUrl(status);
  return addresses.filter(a => developer || !a.developerOnly).map(a => {
    const original = status.receiverUrls.find(url => { try { const parsed = new URL(url); return parsed.protocol === 'https:' && parsed.hostname === a.ipv4; } catch { return false; } });
    const query = status.receiverUrls.flatMap(url => { try { const parsed = new URL(url); return parsed.protocol === 'https:' ? [parsed.search] : []; } catch { return []; } })[0] ?? '';
    const receiver = original ?? `https://${a.ipv4}:${status.receiverPort || 8443}/${query}`;
    const viaName = named !== null && (!status.publicName?.address || status.publicName.address === a.ipv4);
    return { ...a, label: a.developerOnly ? a.interface : status.wifiClient?.setupHotspot || status.network.mode !== 'shared-wifi' ? 'home.hotspot' : status.network.wifi || status.wifiClient?.ssid || 'home.hotspot', host: status.host, receiver: viaName ? named : receiver, fallback: viaName ? receiver : null, admin: `https://${a.ipv4}:${status.adminPort || 6975}/` };
  });
}
/** The head-unit address under the trusted-HTTPS name while the relay trusts it (also during a failing renewal), as the relay lists it. */
export function publicNameUrl(status: Pick<Status, 'publicName' | 'receiverUrls' | 'receiverPort'>): string | null {
  const name = status.publicName?.trustedName;
  if (!name) return null;
  return status.receiverUrls.find(url => { try { return new URL(url).hostname === name; } catch { return false; } }) ?? `https://${name}:${status.receiverPort || 8443}/`;
}
const technicalFields = new Set(['carlinkModel', 'carlinkHevc', 'receiverPort', 'wifiInterface', 'rfcommChannel', 'wifiChannel', 'hotspotChannel', 'hotspotBand', 'wifiSsid', 'wifiPassword', 'hotspotSsid', 'hotspotPassword', 'carlinkLink', 'carlinkInterface', 'carlinkAddress', 'carlinkHeadUnit', 'carlinkLocal', 'transport', 'displayMode', 'displayWidth', 'displayHeight', 'displayDensity']);
export function settingsViews(settings: Settings, developer: boolean, platform?: Status['platform']) {
  const all = settings.schema.filter(s => s.id !== 'wifi' && s.id !== 'plugins' && s.id !== 'publicName').flatMap(s => s.fields).filter(f => platform !== 'linux' || f.key !== 'hotspotBand').map(field => {
    if (field.key !== 'networkMode') return field;
    const options = field.options?.filter(o => developer || o.value === 'shared-wifi' || o.value === 'device-hotspot' || o.value === 'wifi-direct').map(option => {
      const mode = option.value === 'wifi-direct' ? 'device-hotspot' : String(option.value);
      return { ...option, labelKey: `option.networkMode.${mode}`, hintKey: `hint.option.networkMode.${mode}`, illustration: mode as 'shared-wifi' | 'device-hotspot' };
    });
    return { ...field, options };
  });
  const connection = new Set(['iphone', 'networkMode', 'devicePreset', 'sharedWifiNetworks', 'autoStart', 'bootStart']);
  return [
    { id: 'connection', titleKey: 'settings.connection', fields: all.filter(f => connection.has(f.key) && !technicalFields.has(f.key)) },
    { id: 'output', titleKey: 'settings.screen', fields: all.filter(f => !connection.has(f.key) && !technicalFields.has(f.key)) },
    // Trusted HTTPS serves the car screen's page, so it follows the Car screen section (Linux only).
    ...settings.schema.filter(s => s.id === 'publicName'),
    ...settings.schema.filter(s => s.id === 'plugins'),
    ...(developer ? [{ id: 'developer', titleKey: 'settings.developer', fields: all.filter(f => technicalFields.has(f.key)) }] : []),
  ];
}
export function developerPreference(search: string, read: () => string | null): boolean {
  if (new URLSearchParams(search).get('dev') === '1') return true;
  try { return read() === '1'; } catch { return false; }
}

// Navigation links, collapsed menus and modal dialogs are outside the resting Home button budget.
export function homeButtonCount(status: Status, defaultPassword = false): number {
  const adminUrls = status.platform === 'linux' ? status.adminUrls : undefined;
  return 1 + (homeView(status, undefined, defaultPassword).action ? 1 : 0) + (addressViews(status, false).length ? adminUrls ? 2 : 3 : 0) + (adminUrls?.length ?? 0);
}

export function savedNetworks(settings: Settings): SavedNetwork[] { return Array.isArray(settings.values.sharedWifiNetworks) ? settings.values.sharedWifiNetworks as SavedNetwork[] : []; }
export function networkPanelView(status: Status, settings: Settings) {
  const shared = status.network.mode === 'shared-wifi';
  const list = savedNetworks(settings);
  const setup = shared && status.wifiClient?.setupHotspot === true;
  return { shared, list, full: list.length >= 10, needsNetwork: !shared && !list.length,
    mode: status.network.mode, label: shared ? 'net.shared' : 'net.hotspot',
    target: shared ? (status.platform === 'linux' ? 'device-hotspot' : 'wifi-direct') : 'shared-wifi',
    statusKey: shared ? setup ? 'net.setup' : status.network.wifi ? 'net.connected' : 'net.connecting' : status.hotspot?.active ? 'net.on' : 'net.starts',
    parameters: { ssid: setup ? status.hotspot?.ssid ?? '—' : status.network.wifi ?? '—', ip: status.wifiClient?.ip ?? status.hostAddresses?.find(a => !a.developerOnly)?.ipv4 ?? '—' },
    receiver: status.hotspot?.ip ? `https://${status.hotspot.ip}:${status.receiverPort || 8443}` : null };
}
export function addSavesAndConnects(status: Status): boolean {
  return status.wifiControl === true && status.network.mode === 'shared-wifi' && (!status.network.wifi || status.wifiClient?.setupHotspot === true);
}
export function savedNetworkTags(entry: SavedNetwork, status: Status, scan: WifiNetwork[]): string[] {
  return status.network.mode === 'shared-wifi' && !status.wifiClient?.setupHotspot && status.network.wifi === entry.ssid ? ['net.current'] : scan.some(n => n.ssid === entry.ssid) ? ['net.nearby'] : [];
}
export function canConnectNetwork(entry: SavedNetwork, status: Status): boolean {
  return status.wifiControl === true && status.network.mode === 'shared-wifi' && (status.network.wifi !== entry.ssid || status.wifiClient?.setupHotspot === true);
}
export function switchConsequences(status: Status, settings: Settings, target: string) {
  const shared = target === 'shared-wifi';
  const parameters = { wifi: shared ? savedNetworks(settings)[0]?.ssid ?? '' : status.network.wifi ?? '',
    ssid: status.hotspot?.ssid ?? String(settings.values.hotspotSsid ?? '—'),
    password: status.hotspot?.password ?? String(settings.values.hotspotPassword ?? '—'),
    url: `https://${status.hotspot?.ip || status.host}:${status.adminPort || 6975}` };
  return { shared, parameters, title: shared ? 'net.confirmShared' : 'net.confirmHotspot',
    action: shared ? 'net.switchShared' : 'net.confirmAction',
    change: shared ? 'net.closeHotspot' : status.network.wifi ? 'net.leaveWifi' : 'net.startHotspot',
    car: shared ? 'net.carShared' : 'net.carPassword', comeback: shared ? 'net.returnShared' : 'net.returnHotspot' };
}
