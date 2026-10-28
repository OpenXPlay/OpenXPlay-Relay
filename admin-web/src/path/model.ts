// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-27

export const sections = ['RELAY', 'CAR', 'IPHONE', 'PICTURE'] as const;
export type PathSection = typeof sections[number];
export type StepState = 'OK' | 'WORKING' | 'WAITING' | 'ACTION' | 'FAILED' | 'BLOCKED';
export const stepInfo = {
  "MODE": {
    "section": "RELAY",
    "code": "MODE"
  },
  "IDENTITY": {
    "section": "RELAY",
    "code": "ID"
  },
  "PERMISSIONS": {
    "section": "RELAY",
    "code": "PERM"
  },
  "KEEP_ALIVE": {
    "section": "RELAY",
    "code": "BG"
  },
  "BLUETOOTH": {
    "section": "RELAY",
    "code": "BT"
  },
  "WIFI": {
    "section": "RELAY",
    "code": "WIFI"
  },
  "HOTSPOT": {
    "section": "RELAY",
    "code": "HS"
  },
  "SHARED_WIFI": {
    "section": "RELAY",
    "code": "LAN"
  },
  "CAR_NETWORK": {
    "section": "CAR",
    "code": "NET"
  },
  "WEB_SERVER": {
    "section": "CAR",
    "code": "WEB"
  },
  "CAR_PAGE": {
    "section": "CAR",
    "code": "PAGE"
  },
  "CARLINK_FIND": {
    "section": "CAR",
    "code": "BLE"
  },
  "CARLINK_LINK": {
    "section": "CAR",
    "code": "LINK"
  },
  "CARLINK_PIN": {
    "section": "CAR",
    "code": "PIN"
  },
  "CARLINK_READY": {
    "section": "CAR",
    "code": "CL"
  },
  "DLNA": {
    "section": "CAR",
    "code": "DLNA"
  },
  "MIRACAST": {
    "section": "CAR",
    "code": "MC"
  },
  "IPHONE_PAIRED": {
    "section": "IPHONE",
    "code": "PAIR"
  },
  "USB_HOST": {
    "section": "IPHONE",
    "code": "USB"
  },
  "CARPLAY_START": {
    "section": "IPHONE",
    "code": "CP"
  },
  "IPHONE_BLUETOOTH": {
    "section": "IPHONE",
    "code": "RF"
  },
  "IPHONE_AUTH": {
    "section": "IPHONE",
    "code": "IAP"
  },
  "IPHONE_WIFI": {
    "section": "IPHONE",
    "code": "JOIN"
  },
  "USB_IPHONE": {
    "section": "IPHONE",
    "code": "DEV"
  },
  "USB_TRUST": {
    "section": "IPHONE",
    "code": "TRUST"
  },
  "USB_CONTROL": {
    "section": "IPHONE",
    "code": "CTRL"
  },
  "SESSION": {
    "section": "IPHONE",
    "code": "SES"
  },
  "TESLA": {
    "section": "PICTURE",
    "code": "TSLA"
  },
  "PICTURE": {
    "section": "PICTURE",
    "code": "LIVE"
  }
} as const;
export type StepId = keyof typeof stepInfo;
export const actions = ["FIX_SETTINGS", "IMPORT_IDENTITY", "GRANT_PERMISSIONS", "KEEP_ALIVE_SETTINGS", "TURN_ON_BLUETOOTH", "OPEN_WIFI", "OPEN_HOTSPOT", "CHOOSE_SHARED_WIFI", "COPY_PAGE_ADDRESS", "RECONNECT_CARLINK", "ENTER_PIN", "CHOOSE_RENDERER", "OPEN_CAST", "PAIR_IPHONE", "CHOOSE_IPHONE", "START_NOW", "RETRY"] as const;
export type StepAction = typeof actions[number];
export interface StepText { res?: string | null; args?: string[]; raw?: string | null }
export interface PathStep {
  id: StepId; state: StepState; title: string; detail: StepText | null; evidence: StepText | null;
  code: string | null; action: StepAction | null; actionLabel: string | null; sinceMillis: number | null;
  explanation: string | null; trail: string[]; carWaitUntilMillis: number | null;
}
export interface SectionView { section: PathSection; steps: PathStep[]; done: number; total: number; state: StepState }
export interface ConnectionPath {
  sections: SectionView[]; focus: PathStep | null; tone: 'IDLE' | 'READY' | 'PROGRESS' | 'ACTION' | 'ERROR' | 'LIVE';
  canStart: boolean; running: boolean; live: { sinceMillis: number | null; chips: string[] } | null;
  nodes: { iphone: string | null; relay: string | null; car: string | null }; diagnostic: string; version: string;
}
export interface OutputInput { phase: 'OFF' | 'WAITING_USER' | 'SEARCHING' | 'READY' | 'WAITING_VIDEO' | 'STARTING' | 'PLAYING' | 'FAILED'; name: string | null; code: string | null; sinceMillis: number | null }
export interface CarlinkInput {
  phase: 'OFF' | 'UNSUPPORTED' | 'NEEDS_PERMISSIONS' | 'BLOCKED' | 'SCANNING' | 'EXCHANGING' | 'JOINING' | 'AUTHENTICATING' | 'PIN' | 'NEGOTIATING' | 'PLAYING' | 'ENDED';
  sinceMillis: number | null; code: string | null; error: string | null; retrying: boolean; carName: string | null;
  joinMethod: string | null; trusted: boolean; pinPending: boolean; picture: string | null; trail: string[];
}
export type LinkStep = 'IDLE' | 'STARTING' | 'WAITING_IPHONE' | 'BLUETOOTH' | 'AUTH' | 'WIFI_SENT' | 'JOINING' | 'USB_DETECT' | 'USB_PERMISSION' | 'USB_TRUST' | 'USB_CONTROL' | 'SESSION' | 'VIDEO' | 'ENDED';
export interface LinkFailure { step: LinkStep; code: string; text: string; atMillis: number; retrying: boolean; trail: string[] }
export interface PathInput {
  platform: 'android' | 'linux'; nowMillis: number; version: string;
  startup?: import('../api').RuntimeStartup;
  setup: { network: 'WIFI_DIRECT' | 'PHONE_HOTSPOT' | 'SHARED_WIFI' | 'USB'; outputs: ('BROWSER' | 'CARLINK' | 'DLNA' | 'MIRACAST' | 'TESLA')[]; summary: string; valid: boolean };
  relay: { identity: 'READY' | 'MISSING' | 'CHECKING'; identityLabel: string | null; permissionsGranted: number; permissionsTotal: number; vpnConsentNeeded: boolean; keepAlive: 'NOT_NEEDED' | 'PENDING' | 'FOREGROUND' | 'REFUSED'; bluetoothOn: boolean | null; bluetoothName: string | null; wifiOn: boolean | null; wifiSsid: string | null; hotspotOn: boolean | null; hotspotSsid: string | null; hotspotBand: string | null; sharedWifiSsid: string | null; overlayNeeded: boolean };
  car: { displayMismatch: boolean; address: string | null; web: { phase: 'OFF' | 'STARTING' | 'LISTENING' | 'FAILED'; port: number | null; code: string | null; trustedName: string | null; sinceMillis: number | null }; pageUrl: string | null; pages: number; screen: string | null; savedScreen: string | null; carlink: CarlinkInput; dlna: OutputInput; miracast: OutputInput; tesla: OutputInput & { activation?: 'OFF' | 'NEEDS_BROWSER' | 'NEEDS_RELAY_HOTSPOT' | 'NEEDS_NAME' | 'READY' | null } };
  iphone: { pick: 'NONE' | 'PICKED' | 'UNPAIRED' | 'MULTIPLE' | 'NOT_IPHONE'; name: string | null; usb: 'HOST' | 'DEVICE' | 'DISCONNECTED' | 'UNKNOWN'; started: boolean; armed: boolean; carWaitUntilMillis: number | null; step: LinkStep; stepSinceMillis: number | null; stepDetail: string | null; failure: LinkFailure | null };
  live: { pictureOnCar: boolean; sessionSinceMillis: number | null; chips: string[] };
}
type PartialFacts<T> = T extends unknown[] ? T : T extends object ? { [K in keyof T]?: PartialFacts<T[K]> } : T;
export type PartialInput = PartialFacts<PathInput>;
const output: OutputInput = { phase: 'OFF', name: null, code: null, sinceMillis: null };
export function normalizeInput(input: PartialInput = {}): PathInput {
  return {
    platform: input.platform ?? 'android', nowMillis: input.nowMillis ?? 0, version: input.version ?? '',
    startup: input.startup ? { automatic: false, bootStart: false, paused: false, ...input.startup, arrival: { pending: false, deadlineMillis: null, remainingMillis: null, ...input.startup.arrival } } : undefined,
    setup: { network: 'WIFI_DIRECT', outputs: ['BROWSER'], summary: '', valid: true, ...input.setup },
    relay: { identity: 'CHECKING', identityLabel: null, permissionsGranted: 0, permissionsTotal: 0, vpnConsentNeeded: false, keepAlive: 'NOT_NEEDED', bluetoothOn: null, bluetoothName: null, wifiOn: null, wifiSsid: null, hotspotOn: null, hotspotSsid: null, hotspotBand: null, sharedWifiSsid: null, overlayNeeded: false, ...input.relay },
    car: { displayMismatch: false, address: null, pageUrl: null, pages: 0, screen: null, savedScreen: null, ...input.car,
      web: { phase: 'OFF', port: null, code: null, trustedName: null, sinceMillis: null, ...input.car?.web },
      carlink: { phase: 'OFF', sinceMillis: null, code: null, error: null, retrying: false, carName: null, joinMethod: null, trusted: false, pinPending: false, picture: null, trail: [], ...input.car?.carlink },
      dlna: { ...output, ...input.car?.dlna }, miracast: { ...output, ...input.car?.miracast }, tesla: { ...output, ...input.car?.tesla } },
    iphone: { pick: 'NONE', name: null, usb: 'UNKNOWN', started: false, armed: false, carWaitUntilMillis: null, step: 'IDLE', stepSinceMillis: null, stepDetail: null, ...input.iphone, failure: input.iphone?.failure ? { step: 'IDLE', code: 'unknown', text: '', atMillis: 0, retrying: false, trail: [], ...input.iphone.failure } : null },
    live: { pictureOnCar: false, sessionSinceMillis: null, chips: [], ...input.live },
  };
}
export function sectionView(section: PathSection, steps: PathStep[]): SectionView {
  const state = (['FAILED', 'ACTION', 'WORKING', 'WAITING', 'BLOCKED'] as const).find(state => steps.some(step => step.state === state)) ?? 'OK';
  return { section, steps, done: steps.filter(step => step.state === 'OK').length, total: steps.length, state };
}
