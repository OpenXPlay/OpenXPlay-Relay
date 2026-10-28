// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-15

import type { Settings, Values, NetworkRolesSetting } from './schema';
export type SessionState = 'login' | 'ready';
export interface RuntimeStartup {
  automatic: boolean; bootStart: boolean; paused: boolean;
  arrival: { pending: boolean; deadlineMillis: number | null; remainingMillis: number | null };
}
export interface Session { state: SessionState; defaultPassword: boolean }
export interface WifiNetwork { ssid: string; signal: number; secure: boolean; known: boolean }
export interface WifiClient { ssid: string | null; ip: string | null; signal: number | null; setupHotspot?: boolean }
export interface CarlinkStatus {
  trusted?: boolean; state: string; detail: string; lastLostReason?: string | null; lastError?: string | null;
  headUnit?: { address: string; name?: string | null } | null;
  network?: { ssid: string; interface: string } | null;
  viewSize?: { width: number; height: number } | null;
  stats?: Record<string, string | number | null> | null; pinIncorrect?: boolean;
  pinRequested?: boolean; pinDeadline?: number | null;
}
export interface HostAddress { interface: string; ipv4: string; developerOnly: boolean }
export interface PairedDevice { name: string; connected: boolean; iphone: boolean }
export interface VideoStatus { codec: string; reason: string; codecString?: string | null; width?: number | null; height?: number | null }
// One browser on the car screen: its latest receiver-info and the most recent per-second stats, oldest first.
// Linux device-support contract (UFI003 Bluetooth guard); absent where no guard runs.
export interface HostProfile { model: string | null; profilePath: string; profileLoaded: boolean; profileError: string | null; bluetoothAdapter: { name: string; address: string } | null; wifiInterface: string; wifiPresent: boolean; secureConnections: 'pairing-off' | 'untouched' }
export interface HostChannel { frequency: number; channel: number; disabled: boolean; noIr: boolean; radar: boolean }
export interface HostWifi { interface: string; present: boolean | null; phy: string | null; capabilities: { modes: string[]; ap: boolean; station: boolean; combinations: { apStation: boolean; channels: number | null }[]; channels: HostChannel[] } | null }
export interface HostCapabilities {
  wifi: HostWifi; carlink: HostWifi; regulatory: Record<string, string>;
  adapters: { name: string; address: string; powered: boolean | null }[] | null; selectedAdapter: string | null;
  bluetooth: { brEdr: boolean; le: boolean; secureConnections: boolean } | null; rfcomm: boolean | null;
  tools: Record<string, { present: boolean | null; version: string | null }>;
}
export type HotspotBand = 'auto' | '2.4' | '5';
export interface Wireless {
  preset: { id: string; label: string; source: string; detected: string };
  mode: 'concurrent' | 'single-radio' | 'unknown';
  station24: boolean | null; ap24: boolean | null; station5: boolean | null; ap5: boolean | null;
  apStation: boolean | null; apStationChannels: number | null; p2pClient: boolean | null; ap5Channels: number[] | null;
  hotspot: { band: '2.4' | '5'; autoBand: '2.4' | '5'; channel: number; configuredBand: HotspotBand; configuredChannel: number; reason: string | null };
}
export interface PlannedRole { interface: { ref: string; name: string | null; mac: string | null } | null; missing: boolean; source: 'auto' | 'custom' | 'legacy' | 'cli' }
export interface NetworkPlan {
  source: 'auto' | 'custom'; output: string;
  iphone: PlannedRole & { link: 'hotspot' | 'saved-network'; band: string; channel: number };
  carBrowser: PlannedRole & { network: 'iphone' | 'interface' };
  carlink: PlannedRole & { followOver: boolean };
  adminHotspot: PlannedRole & { enabled: boolean; ssid: string; band: string; channel: number; timeSharedWith: 'carlink' | null; passwordSet: boolean };
}
export interface NetworkInterface {
  name: string; mac: string | null; currentMac: string | null; macSource: 'permanent' | 'current'; driver: string | null; bus: string; product: string | null;
  kind: 'wifi' | 'ethernet'; phy: string | null; operstate: string | null; ipv4: string[];
  wifi: { ap: boolean; station: boolean; p2pClient: boolean; p2pGo: boolean; concurrency: string; modes: string[]; channels: HostChannel[];
    combinations: { modes: string[]; channels: number | null; apStation: boolean; description: string }[];
    bands: { band: string; stationChannels: number[]; apChannels: number[] }[] } | null;
  activity: { kind: string; profile: string | null; ssid: string | null; mode: string | null; channel: number | null; width: string | null };
}
export interface NetworkCheck { level: 'error' | 'warning' | 'info'; code: string; roles: string[]; interfaces: string[]; params: Record<string, unknown> }
export interface PlannedNetwork {
  probedAt: number | null;
  country: { global: string | null; configured: string | null; phys: { phy: string; country: string | null; selfManaged: boolean }[] };
  interfaces: NetworkInterface[]; plan: NetworkPlan; checks: NetworkCheck[]; applied: boolean; probing?: boolean;
}
export interface NetworkProposal { plan: NetworkPlan; checks: NetworkCheck[]; changes: ('controller' | 'carlink' | 'carBrowserAddress' | 'adminHotspot' | 'wifiCountry')[]; applied?: true }
export interface BluetoothGuard { state: 'ok' | 'recovering' | 'failed'; attempts: number; lastRecovery: string | null; lastError: string | null; address: string | null }
export interface ReceiverReport { client: string; idleSeconds: number; info: Record<string, unknown> | null; stats: Record<string, unknown>[] }
// Trusted HTTPS with a free domain name (Linux); times are the relay's wall clock, for display.
export type PublicNameState = 'off' | 'incomplete' | 'waiting-network' | 'updating-address' | 'requesting-certificate' | 'ready' | 'error';
// trustedName: the name while its certificate is valid and the current address is published, also while the status line reports a problem.
export interface PublicNameStatus { state: PublicNameState; name?: string; address?: string; notAfter?: number; error?: string; errorDetail?: string; nextAttempt?: number; fileErrors?: Record<string, string>; trustedName?: string }
export interface Status {
  adminFeatures?: ('logs' | 'config-files' | 'update-check')[];
  pathInput?: import('./path/model').PartialInput & {
    startup?: RuntimeStartup;
    iphone?: import('./path/model').PartialInput['iphone'] & { trail?: string[]; runtimeIap2Frames?: number };
  };
  adminHotspot?: { state: 'up' | 'paused-carlink' | 'waiting-interface' | 'off' | 'failed'; reason: string | null };
  hostAddresses?: HostAddress[]; receiverPort?: number; adminPort?: number; pairingAvailable?: boolean; pairingDeadline?: number | null; screenConnected?: boolean;
  pairingRemaining?: number | null; pairingCancelable?: boolean; pairingNeedsConfirmation?: boolean; pairingOnRelay?: boolean; pairedDevices?: PairedDevice[];
  // Android 12+: the Nearby devices step of a pairing window, and whether the request waits in a notification.
  pairingNeedsPermission?: boolean; pairingPermissionDenied?: boolean; pairingNotified?: boolean;
  video?: VideoStatus; receivers?: ReceiverReport[]; bluetoothGuard?: BluetoothGuard;
  hotspot?: { hasPassword?: boolean; ssid: string; password: string; ip: string | null; active: boolean } | null; wifiControl?: boolean;
  wifiScanStale?: boolean; wifiProvisioning?: boolean; wifiClient?: WifiClient;
  platform: 'android' | 'linux'; name: string; version: string; host: string; hostProfile?: HostProfile; hostCapabilities?: HostCapabilities; adminUrl: string; adminUrls?: string[]; receiverUrls: string[];
  state: 'idle' | 'waiting' | 'connecting' | 'active' | 'reconnecting'; running: boolean; autoStartPaused: boolean;
  iphone: { name: string | null; reason: string }; network: { mode: string; wifi: string | null; followingCarlink?: boolean } & Partial<PlannedNetwork>;
  output: string; carlink: CarlinkStatus | null; notices: string[]; publicName?: PublicNameStatus; wireless?: Wireless;
}
export interface Phone { id: string; name: string; kind: 'phone' | 'car' | 'other'; connected: boolean; likelyIphone: boolean; selected: boolean }
export interface Update { ok: boolean; applied: 'now' | 'next-start' }
export interface PublicNameImport { ok: true; provider: string; name: string }
export class ApiError extends Error {
  constructor(public status: number, public key: string, public errors: Record<string, string> = {}, public retryAfter = 60, public checks: NetworkCheck[] = []) { super(key); }
}
export class NetworkError extends Error {}
export const events = new EventTarget();
export async function request<T>(path: string, method = 'GET', body?: unknown, retry = true, timeoutMillis = 8000): Promise<T> {
  let response: Response;
  try {
    response = await fetch(`/api/v0${path}`, {
      method, credentials: 'same-origin', cache: 'no-store', signal: AbortSignal.timeout(timeoutMillis),
      headers: method === 'GET' ? {} : { 'Content-Type': 'application/json', 'X-OpenXPlay-Admin': '1' },
      ...(method === 'GET' ? {} : { body: JSON.stringify(body ?? {}) }),
    });
  } catch {
    events.dispatchEvent(new Event('offline'));
    if (retry) {
      await new Promise(resolve => setTimeout(resolve, 1000));
      return request<T>(path, method, body, false, timeoutMillis);
    }
    throw new NetworkError('error.network');
  }
  events.dispatchEvent(new Event('online'));
  const data = await response.json().catch(() => ({}));
  if (!response.ok) {
    if (response.status === 401) events.dispatchEvent(new Event('unauthorized'));
    throw new ApiError(response.status, response.status === 429 ? 'error.rateLimit' : data.error ?? 'error.request', data.errors ?? {}, Number(response.headers.get('Retry-After')) || 60, data.checks || []);
  }
  return data as T;
}
export const api = {
  logExport: (range: '10m' | '30m' | '24h') => request<import('./feature-pages').LogExport>('/logs/export', 'POST', { range }, false, 60000),
  logUpload: (id: string) => request<import('./feature-pages').LogReceipt>('/logs/upload', 'POST', { id }, false, 180000),
  logReceipt: () => request<import('./feature-pages').LogReceipt>('/logs/receipt', 'GET', undefined, false),
  configImport: (envelope: unknown, dryRun = false) => request<import('./feature-pages').ConfigImport>('/config/import' + (dryRun ? '?dryRun=1' : ''), 'POST', envelope, false),
  updateCheck: async (channel: 'stable' | 'beta') => { const result = await request<import('./feature-pages').UpdateCheck | { error: string }>('/update?channel=' + channel, 'GET', undefined, false, 15000); if ('error' in result) throw new ApiError(503, result.error); return result; },
  previewRoles: (networkRoles: NetworkRolesSetting, extra: { wifiCountry?: string; adminHotspotPassword?: string } = {}) => request<NetworkProposal>('/network-roles/preview', 'POST', { networkRoles, ...extra }, false),
  saveRoles: (networkRoles: NetworkRolesSetting, extra: { wifiCountry?: string; adminHotspotPassword?: string } = {}) => request<NetworkProposal>('/network-roles', 'PUT', { networkRoles, ...extra }, false),
  session: () => request<Session>('/session'),
  login: (password: string) => request('/login', 'POST', { password }),
  password: (current: string, next: string) => request('/password', 'POST', { current, new: next }, false),
  networks: () => request<WifiNetwork[]>('/wifi/networks'),
  wifi: (ssid: string, password?: string) => request('/wifi', 'PUT', { ssid, ...(password === undefined ? {} : { password }) }, false),
  hotspot: (ssid: string, password: string, band?: HotspotBand) => request('/hotspot', 'PUT', { ssid, password, ...(band === undefined ? {} : { band }) }, false),
  logout: () => request('/logout', 'POST'),
  // Receiver diagnostics are large and change every second; only the developer section asks for them.
  status: (diagnostics = false) => request<Status>(diagnostics ? '/status?diagnostics=1' : '/status'),
  settings: () => request<Settings>('/settings'),
  publicNameImport: (content: string) => request<PublicNameImport>('/public-name/import', 'POST', { content }, false),
  save: (values: Values) => request<Update>('/settings', 'PUT', values, false),
  switchNetwork: (networkMode: string) => request<Update>('/settings', 'PUT', { networkMode }, false),
  carlinkConnect: (pin?: string) => request('/carlink/connect', 'POST', pin === undefined ? {} : { pin }, false),
  carlinkPin: (pin: string) => request('/carlink/pin', 'POST', { pin }, false),
  action: (name: 'start' | 'stop' | 'cancel-arrival' | 'restart' | 'regenerate-pairing-code' | 'forget-wifi' | 'forget-carlink' | 'clear-saved-head-unit' | 'pair-iphone' | 'cancel-pairing' | 'public-name-renew' | 'public-name-remove') => request(`/actions/${name}`, 'POST', {}, !['forget-wifi', 'forget-carlink'].includes(name)),
  phones: () => request<Phone[]>('/phones'),
};
