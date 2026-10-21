// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-15

import type { Settings, Values } from './schema';
export type SessionState = 'login' | 'ready';
export interface Session { state: SessionState; defaultPassword: boolean }
export interface WifiNetwork { ssid: string; signal: number; secure: boolean; known: boolean }
export interface WifiClient { ssid: string | null; ip: string | null; signal: number | null; setupHotspot?: boolean }
export interface CarlinkStatus {
  trusted?: boolean; state: string; detail: string; lastLostReason?: string | null; lastError?: string | null;
  headUnit?: { address: string; name?: string | null } | null;
  network?: { ssid: string; interface: string } | null;
  pinRequested?: boolean; pinDeadline?: number | null;
}
export interface HostAddress { interface: string; ipv4: string; developerOnly: boolean }
export interface PairedDevice { name: string; connected: boolean; iphone: boolean }
export interface VideoStatus { codec: string; reason: string; codecString?: string | null; width?: number | null; height?: number | null }
// One browser on the car screen: its latest receiver-info and the most recent per-second stats, oldest first.
// Linux device-support contract (UFI003 Bluetooth guard); absent where no guard runs.
export interface HostProfile { model: string | null; profilePath: string; profileLoaded: boolean; profileError: string | null; bluetoothAdapter: { name: string; address: string } | null; wifiInterface: string; wifiPresent: boolean; secureConnections: 'pairing-off' | 'untouched' }
export interface BluetoothGuard { state: 'ok' | 'recovering' | 'failed'; attempts: number; lastRecovery: string | null; lastError: string | null; address: string | null }
export interface ReceiverReport { client: string; idleSeconds: number; info: Record<string, unknown> | null; stats: Record<string, unknown>[] }
export interface Status {
  hostAddresses?: HostAddress[]; receiverPort?: number; adminPort?: number; pairingAvailable?: boolean; pairingDeadline?: number | null; screenConnected?: boolean;
  pairingRemaining?: number | null; pairingCancelable?: boolean; pairingNeedsConfirmation?: boolean; pairingOnRelay?: boolean; pairedDevices?: PairedDevice[];
  // Android 12+: the Nearby devices step of a pairing window, and whether the request waits in a notification.
  pairingNeedsPermission?: boolean; pairingPermissionDenied?: boolean; pairingNotified?: boolean;
  video?: VideoStatus; receivers?: ReceiverReport[]; bluetoothGuard?: BluetoothGuard;
  hotspot?: { hasPassword?: boolean; ssid: string; password: string; ip: string | null; active: boolean } | null; wifiControl?: boolean;
  wifiScanStale?: boolean; wifiProvisioning?: boolean; wifiClient?: WifiClient;
  platform: 'android' | 'linux'; name: string; version: string; host: string; hostProfile?: HostProfile; adminUrl: string; receiverUrls: string[];
  state: 'idle' | 'waiting' | 'connecting' | 'active' | 'reconnecting'; running: boolean; autoStartPaused: boolean;
  iphone: { name: string | null; reason: string }; network: { mode: string; wifi: string | null; followingCarlink?: boolean };
  output: string; carlink: CarlinkStatus | null; notices: string[];
}
export interface Phone { id: string; name: string; kind: 'phone' | 'car' | 'other'; connected: boolean; likelyIphone: boolean; selected: boolean }
export interface Update { ok: boolean; applied: 'now' | 'next-start' }
export class ApiError extends Error {
  constructor(public status: number, public key: string, public errors: Record<string, string> = {}, public retryAfter = 60) { super(key); }
}
export class NetworkError extends Error {}
export const events = new EventTarget();
export async function request<T>(path: string, method = 'GET', body?: unknown, retry = true): Promise<T> {
  let response: Response;
  try {
    response = await fetch(`/api/v0${path}`, {
      method, credentials: 'same-origin', cache: 'no-store', signal: AbortSignal.timeout(8000),
      headers: method === 'GET' ? {} : { 'Content-Type': 'application/json', 'X-OpenXPlay-Admin': '1' },
      ...(method === 'GET' ? {} : { body: JSON.stringify(body ?? {}) }),
    });
  } catch {
    events.dispatchEvent(new Event('offline'));
    if (retry) {
      await new Promise(resolve => setTimeout(resolve, 1000));
      return request<T>(path, method, body, false);
    }
    throw new NetworkError('error.network');
  }
  events.dispatchEvent(new Event('online'));
  const data = await response.json().catch(() => ({}));
  if (!response.ok) {
    if (response.status === 401) events.dispatchEvent(new Event('unauthorized'));
    throw new ApiError(response.status, response.status === 429 ? 'error.rateLimit' : data.error ?? 'error.request', data.errors ?? {}, Number(response.headers.get('Retry-After')) || 60);
  }
  return data as T;
}
export const api = {
  session: () => request<Session>('/session'),
  login: (password: string) => request('/login', 'POST', { password }),
  password: (current: string, next: string) => request('/password', 'POST', { current, new: next }, false),
  networks: () => request<WifiNetwork[]>('/wifi/networks'),
  wifi: (ssid: string, password?: string) => request('/wifi', 'PUT', { ssid, ...(password === undefined ? {} : { password }) }, false),
  hotspot: (ssid: string, password: string) => request('/hotspot', 'PUT', { ssid, password }, false),
  logout: () => request('/logout', 'POST'),
  // Receiver diagnostics are large and change every second; only the developer section asks for them.
  status: (diagnostics = false) => request<Status>(diagnostics ? '/status?diagnostics=1' : '/status'),
  settings: () => request<Settings>('/settings'),
  save: (values: Values) => request<Update>('/settings', 'PUT', values, false),
  switchNetwork: (networkMode: string) => request<Update>('/settings', 'PUT', { networkMode }, false),
  carlinkConnect: (pin?: string) => request('/carlink/connect', 'POST', pin === undefined ? {} : { pin }, false),
  carlinkPin: (pin: string) => request('/carlink/pin', 'POST', { pin }, false),
  action: (name: 'start' | 'stop' | 'restart' | 'regenerate-pairing-code' | 'forget-wifi' | 'forget-carlink' | 'pair-iphone' | 'cancel-pairing') => request(`/actions/${name}`, 'POST', {}, !['forget-wifi', 'forget-carlink'].includes(name)),
  phones: () => request<Phone[]>('/phones'),
};
