// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-27

import { updateMenuVersion } from './update-check';
import { isVisible, optionLabel, type Field, type Settings } from './schema';
import type { Status } from './api';
import type { Language } from './i18n';
export const pageKeys: Record<string, string[]> = {
  output: ['output', 'wirelessDisplay', 'carlinkName', 'vehicleInformation', 'carlinkAutoReconnect', 'carlinkReconnectOnResize'],
  vehicle: [],
  'output/plugins': ['plugins', 'pluginsEnabled'],
  tesla: ['teslaBrowser', 'teslaVirtualAddress', 'teslaPublicAddress', 'teslaMirrorAddress', 'teslaAndroidAddress'],
  car: ['carBrand', 'carBrandText', 'carModel', 'carModelText', 'carYear', 'carYearText', 'savedHeadUnit', 'savedScreen', 'carPlayViewAreas', 'viewAreas'],
  picture: ['carplayResolution', 'resolution', 'carplayScreenWidth', 'screenWidth', 'frameRate', 'videoCodec', 'videoStats', 'carlinkPictureSizes', 'carlinkPictureDefault', 'carlinkPictureTry'],
  audio: ['carBluetoothAudio', 'microphone', 'microphoneSource'],
  connection: ['iphone', 'transport'],
  'connection/network': ['networkMode', 'hotspotSsid', 'hotspotPassword', 'hotspotBand', 'wifiSsid', 'wifiPassword', 'wifiCountry', 'sharedWifiNetworks'],
  'connection/network/interfaces': ['networkRoles', 'wifiInterface', 'carlinkInterface', 'adminHotspotPassword'],
  startup: ['autoStart', 'bootStart'],
  access: ['requirePairingCode', 'adminPort', 'webPort'],
  'access/public-name': ['publicNameEnabled', 'publicNameProvider', 'publicName', 'publicNameToken', 'publicNameAgreeTerms'],
  'developer/carlink': ['physicalWidthEnabled', 'physicalWidthOverrideMm', 'carlinkModel', 'carlinkHevc', 'carlinkTrace', 'carlinkLink', 'carlinkAddress', 'carlinkHeadUnit', 'carlinkLocal', 'carlinkPictureSize'],
  'developer/video': ['displayMode', 'displayWidth', 'displayHeight', 'displayDensity', 'fps'],
  'developer/connection': ['receiverPort', 'rfcommChannel', 'hotspotChannel', 'hotspotSecurity', 'wifiChannel', 'devicePreset'],
  'identity': ['identity', 'identitySource', 'tlsCertificate', 'mfiTarget', 'mfiI2cPath', 'mfiRemoteServer', 'mfiRemoteToken'],
  developer: ['developerActive'],
  'experimental': ['experimentalScreenSessionIsolation', 'experimentalScreenReconnectReset', 'experimentalScreenDeliverySerialization', 'experimentalRemotePlaybackProof', 'experimentalStartupStatus', 'experimentalStartupDiagnostics', 'experimentalActive', 'experimentalSendBluetoothAddress', 'experimentalBluetoothAddress'],
};
export function fieldEnabled(key: string, values: Settings['values']): boolean {
  if (key.startsWith('experimental') && key !== 'experimentalActive') return values.experimentalActive !== false;
  return !['physicalWidthEnabled', 'physicalWidthOverrideMm', 'carlinkTrace', 'carlinkHevc', 'carlinkPictureSize', 'carlinkModel', 'receiverPort', 'rfcommChannel', 'devicePreset', 'carlinkLink', 'carlinkAddress', 'carlinkHeadUnit', 'carlinkLocal'].includes(key) || values.developerActive !== false;
}
export function fieldRoute(key: string): string {
  return 'settings/' + (Object.entries(pageKeys).find(([, keys]) => keys.includes(key))?.[0] ?? 'developer/other');
}
export function pageFields(settings: Settings, route: string): Field[] { return settings.schema.flatMap(s => s.fields).filter(f => fieldRoute(f.key) === route); }
export const pages = [
  ['output', 'menu.output', 'car'], ['car', 'menu.car', 'car'], ['tesla', 'menu.tesla', 'car'], ['picture', 'menu.picture', 'monitor'], ['audio', 'menu.audio', 'bluetooth'],
  ['connection', 'menu.connection', 'smartphone'], ['startup', 'menu.startup', 'play'], ['access', 'menu.access', 'lock'],
  ['identity', 'menu.identity', 'file-key-2'], ['config-files', 'menu.config-files', 'code'], ['logs', 'menu.logs', 'file-text'], ['about', 'menu.about', 'info'],
  ['developer', 'menu.developer', 'code'], ['experimental', 'menu.experimental', 'settings'],
] as const;
export const developerPages = ['diagnostics', 'carlink', 'video', 'connection', 'other'] as const;
export function routeTitle(route: string): string { return route === 'settings' ? 'ui.settings' : route.startsWith('settings/developer/') ? `menu.developer.${route.split('/').at(-1)}` : `menu.${route.slice(9)}`; }
export function parentRoute(route: string): string { return route.includes('/') ? route.slice(0, route.lastIndexOf('/')) : 'home'; }
export function normalizeRoute(hash: string): string {
  let route = hash.replace(/^#\/?/, '') || 'home';
  if (['settings/developer/identity', 'developer/identity', 'settings:identity'].includes(route)) route = 'settings/identity';
  const valid = ['home', 'settings', ...pages.map(([p]) => `settings/${p}`), ...Object.keys(pageKeys).map(p => `settings/${p}`), ...developerPages.map(p => `settings/developer/${p}`), 'settings/access/admin'];
  return valid.includes(route) ? route : 'settings';
}
export function settingsPlacement(settings: Settings) { return settings.schema.flatMap(s => s.fields).map(field => ({ field, route: fieldRoute(field.key) })); }
export function restartsPicture(key: string): boolean { return ['carplayResolution', 'carplayScreenWidth', 'resolution', 'screenWidth', 'videoCodec', 'frameRate'].includes(key); }

export const developerIcons: Record<string, string> = { diagnostics: 'info', carlink: 'car', video: 'settings', connection: 'wifi', identity: 'file-key-2', other: 'sliders-horizontal' };
export function pageAvailable(settings: Settings, status: Status, route: string): boolean {
  if (route === 'settings/logs' || route === 'settings/config-files') return status.adminFeatures?.includes(route.slice(9) as 'logs' | 'config-files') ?? false;
  if (['settings', 'settings/about', 'settings/access', 'settings/access/admin', 'settings/connection', 'settings/connection/network', 'settings/developer', 'settings/developer/diagnostics', 'settings/developer/connection'].includes(route)) return true;
  if (route === 'settings/identity') return status.adminFeatures?.includes('identity') ?? pageFields(settings, route).length > 0;
  if (route === 'settings/vehicle') return status.platform === 'linux';
  if (route === 'settings/car') return status.platform === 'linux' || pageFields(settings, route).length > 0;
  if (route === 'settings/connection/network/interfaces') return status.platform === 'linux';
  return pageFields(settings, route).some(field => isVisible(field, settings.values));
}
export function autoFrameRateSummary(status: Status, t: (key: string) => string): string {
  const fps = status.running ? status.video?.appliedFps : status.video?.requestedFps;
  return [t('menu.rateAuto'), fps ? `${fps} fps` : ''].filter(Boolean).join(' · ');
}
export function menuSummary(settings: Settings, status: Status, route: string, language: Language, t: (key: string) => string): string {
  const all = settings.schema.flatMap(s => s.fields), v = settings.values;
  const label = (key: string) => {
    const field = all.find(f => f.key === key), value = v[key];
    if (!field || value === '' || value == null) return '';
    return field.type === 'choice' ? optionLabel(field.options?.find(o => o.value === value), language, t) : field.type === 'bool' ? t(value ? 'stage.on' : 'stage.off') : typeof value === 'string' || typeof value === 'number' ? String(value) : '';
  };
  if (route === 'settings/car') return ['carBrand', 'carModel', 'carYear'].map(key => v[key] === 'other' && v[key + 'Text'] ? String(v[key + 'Text']) : label(key)).filter(Boolean).join(' · ');
  if (route === 'settings/picture') {
    const rate = v.frameRate === 'auto' ? autoFrameRateSummary(status, t) : label('frameRate');
    return [label('resolution') || label('carplayResolution'), `${v.screenWidth ?? v.carplayScreenWidth ?? 200} mm`, label('videoCodec'), rate].filter(Boolean).join(' · ');
  }
  if (route === 'settings/tesla') return label('teslaBrowser');
  if (route === 'settings/output') return label('output');
  if (route === 'settings/audio') return `${t('stage.carBluetooth')}: ${label('carBluetoothAudio')}`;
  if (route === 'settings/startup') return [all.some(f => f.key === 'autoStart') ? `${t('menu.whenReady')}: ${label('autoStart')}` : '', all.some(f => f.key === 'bootStart') ? `${t('menu.atBoot')}: ${label('bootStart')}` : ''].filter(Boolean).join(' · ');
  if (route === 'settings/connection') return t(v.transport === 'usb' ? 'option.transport.usb' : 'option.transport.wireless');
  if (route === 'settings/connection/network') return label('networkMode');
  if (route === 'settings/access') return `${t('menu.admin')}: ${t('stage.on')} · ${t('publicName.title')}: ${t(v.publicNameEnabled ? 'stage.on' : 'stage.off')}`;
  if (route === 'settings/experimental') {
    if (!v.experimentalActive) return t('stage.off');
    const options = all.filter(f => f.key.startsWith('experimental') && f.key !== 'experimentalActive' && f.type === 'bool' && v[f.key]).length;
    return options ? `${t('stage.on')} · ${options} ${t('menu.options')}` : t('stage.on');
  }
  if (route === 'settings/identity') return t(status.identity?.mfi.state === 'READY' ? 'direlay_installed' : status.identity?.mfi.state === 'INVALID' ? 'dev_invalid_identity' : 'direlay_missing');
  if (route === 'settings/about') return updateMenuVersion(status.version, t);
  if (route === 'settings/developer/carlink') return [label('carlinkHevc'), label('carlinkTrace')].filter(Boolean).join(' · ');
  if (route === 'settings/developer/connection') return [label('output'), t(v.transport === 'usb' ? 'option.transport.usb' : 'option.transport.wireless')].filter(Boolean).join(' · ');
  if (route === 'settings/developer/video') return ['custom', 'fixed'].includes(String(v.displayMode)) ? `${v.displayWidth} × ${v.displayHeight} · ${v.displayDensity} dpi` : label('displayMode');
  return '';
}
