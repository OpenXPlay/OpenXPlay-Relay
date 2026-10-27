// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-27

import type { Field, Settings } from './schema';
export const pageKeys: Record<string, string[]> = {
  output: ['output', 'wirelessDisplay', 'carlinkName', 'vehicleInformation', 'carlinkAutoReconnect', 'carlinkReconnectOnResize', 'carlinkPictureSizes', 'carlinkPictureDefault', 'carlinkPictureTry', 'plugins', 'pluginsEnabled'],
  car: ['carBrand', 'carBrandText', 'carModel', 'carModelText', 'carYear', 'carYearText', 'savedHeadUnit', 'savedScreen'],
  picture: ['carplayResolution', 'resolution', 'carplayScreenWidth', 'screenWidth', 'frameRate', 'videoCodec', 'videoStats', 'carPlayViewAreas', 'viewAreas'],
  audio: ['carBluetoothAudio', 'microphone', 'microphoneSource'],
  connection: ['iphone', 'transport'],
  'connection/network': ['networkMode', 'hotspotSsid', 'hotspotPassword', 'hotspotBand', 'wifiSsid', 'wifiPassword', 'wifiCountry', 'sharedWifiNetworks'],
  'connection/network/interfaces': ['networkRoles', 'wifiInterface', 'carlinkInterface', 'adminHotspotPassword'],
  startup: ['autoStart', 'bootStart'],
  access: ['requirePairingCode', 'adminPort', 'webPort'],
  'access/public-name': ['publicNameEnabled', 'publicNameProvider', 'publicName', 'publicNameToken', 'publicNameAgreeTerms'],
  'developer/carlink': ['carlinkModel', 'carlinkHevc', 'carlinkTrace', 'carlinkLink', 'carlinkAddress', 'carlinkHeadUnit', 'carlinkLocal', 'carlinkPictureSize'],
  'developer/video': ['displayMode', 'displayWidth', 'displayHeight', 'displayDensity', 'fps'],
  'developer/connection': ['receiverPort', 'rfcommChannel', 'hotspotChannel', 'wifiChannel', 'devicePreset'],
  'developer/identity': ['identity', 'identitySource', 'tlsCertificate', 'mfiTarget'],
  'experimental': ['experimentalActive', 'experimentalSendBluetoothAddress', 'experimentalBluetoothAddress'],
};
export function fieldRoute(key: string): string {
  return 'settings/' + (Object.entries(pageKeys).find(([, keys]) => keys.includes(key))?.[0] ?? 'developer/other');
}
export function pageFields(settings: Settings, route: string): Field[] { return settings.schema.flatMap(s => s.fields).filter(f => fieldRoute(f.key) === route); }
export const pages = [
  ['output', 'menu.output', 'car'], ['car', 'menu.car', 'car'], ['picture', 'menu.picture', 'monitor'], ['audio', 'menu.audio', 'bluetooth'],
  ['connection', 'menu.connection', 'smartphone'], ['startup', 'menu.startup', 'power'], ['access', 'menu.access', 'lock'],
  ['config-files', 'menu.config-files', 'settings'], ['logs', 'menu.logs', 'settings'], ['about', 'menu.about', 'circle-help'],
  ['developer', 'menu.developer', 'settings'], ['experimental', 'menu.experimental', 'settings'],
] as const;
export const developerPages = ['diagnostics', 'carlink', 'video', 'connection', 'identity', 'other'] as const;
export function routeTitle(route: string): string { return route === 'settings' ? 'ui.settings' : route.startsWith('settings/developer/') ? `menu.developer.${route.split('/').at(-1)}` : `menu.${route.slice(9)}`; }
export function parentRoute(route: string): string { return route.includes('/') ? route.slice(0, route.lastIndexOf('/')) : 'home'; }
export function normalizeRoute(hash: string): string {
  const route = hash.replace(/^#\/?/, '') || 'home';
  const valid = ['home', 'settings', ...pages.map(([p]) => `settings/${p}`), ...Object.keys(pageKeys).map(p => `settings/${p}`), ...developerPages.map(p => `settings/developer/${p}`), 'settings/access/admin'];
  return valid.includes(route) ? route : 'settings';
}
export function settingsPlacement(settings: Settings) { return settings.schema.flatMap(s => s.fields).map(field => ({ field, route: fieldRoute(field.key) })); }
export function restartsPicture(key: string): boolean { return ['carplayResolution', 'carplayScreenWidth', 'resolution', 'screenWidth', 'videoCodec', 'frameRate', 'carlinkPictureDefault', 'displayMode', 'displayWidth', 'displayHeight', 'displayDensity'].includes(key); }
