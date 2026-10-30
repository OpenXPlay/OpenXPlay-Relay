// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-27

import { mfiReadiness } from './mfi';
import { updateMenuVersion } from './update-check';
import { isVisible, optionLabel, type Field, type Settings } from './schema';
import type { Status } from './api';
import type { Language } from './i18n';

const experimentalReleases = {
  '0.2.3': ['experimentalIapChannelWhenOffered', 'experimentalReleaseBluetoothOnRequest', 'experimentalAwaitIphoneReconnect', 'experimentalRadioConflictPrompt', 'experimentalStallWatchdog'],
  '0.2.2': ['experimentalScreenSessionIsolation', 'experimentalScreenReconnectReset', 'experimentalScreenDeliverySerialization'],
  '0.2.1': ['experimentalRemotePlaybackProof', 'experimentalCancelRecoveredRetries', 'experimentalStartupStatus', 'experimentalStartupDiagnostics', 'experimentalNetworkObservationBackoff'],
  '0.2.0': ['experimentalSendBluetoothAddress', 'experimentalBluetoothAddress'],
};
// Releases describe when a choice first shipped on each relay platform.
const experimentalPlatformReleases = {
  android: Object.entries(experimentalReleases),
  linux: [['0.1.6', [...experimentalReleases['0.2.3'], ...experimentalReleases['0.2.2'], 'experimentalRemotePlaybackProof', 'experimentalStartupStatus', 'experimentalStartupDiagnostics', ...experimentalReleases['0.2.0']]]],
} satisfies Record<'android' | 'linux', [string, string[]][]>;

export function experimentalGroups(fields: Field[], platform: Status['platform']) {
  return experimentalPlatformReleases[platform ?? 'android'].map(([version, keys]) => ({ version, fields: keys.flatMap(key => {
    const field = fields.find(field => field.key === key);
    return field ? [field] : [];
  }) })).filter(group => group.fields.length);
}
export function experimentalMarkup(fields: Field[], platform: Status['platform'], renderField: (field: Field) => string): string {
  const master = fields.filter(field => field.key === 'experimentalActive');
  const groups = experimentalGroups(fields, platform);
  return (master.length ? `<section class="inset-group settings-section">${master.map(renderField).join('')}</section>` : '') + groups.map(group => `<section class="inset-group settings-section">${groups.length > 1 ? `<h2>${group.version}</h2>` : ''}${group.fields.map(renderField).join('')}</section>`).join('');
}
export const pageKeys: Record<string, string[]> = {
  output: ['output', 'wirelessDisplay', 'carlinkName', 'vehicleInformation', 'carlinkAutoReconnect', 'carlinkReconnectOnResize'],
  vehicle: [],
  'output/plugins': ['plugins', 'pluginsEnabled'],
  tesla: ['teslaBrowser', 'teslaVirtualAddress', 'teslaPublicAddress', 'teslaMirrorAddress', 'teslaAndroidAddress'],
  car: ['carBrand', 'carBrandText', 'carModel', 'carModelText', 'carYear', 'carYearText', 'savedHeadUnit', 'savedScreen', 'carPlayViewAreas', 'viewAreas'],
  picture: ['carplayResolution', 'resolution', 'carplayScreenWidth', 'screenWidth', 'frameRate', 'videoCodec', 'videoStats', 'carlinkPictureSizes', 'carlinkPictureDefault', 'carlinkPictureTry'],
  audio: ['carBluetoothAudio', 'microphone'],
  connection: ['iphone', 'transport'],
  'connection/network': ['networkMode', 'hotspotSsid', 'hotspotPassword', 'hotspotBand', 'wifiSsid', 'wifiPassword', 'wifiCountry', 'sharedWifiNetworks'],
  'connection/network/interfaces': ['networkRoles', 'wifiInterface', 'carlinkInterface', 'adminHotspotPassword'],
  startup: ['autoStart', 'bootStart'],
  access: ['requirePairingCode', 'adminPort', 'webPort'],
  'access/public-name': ['publicNameEnabled', 'publicNameProvider', 'publicName', 'publicNameToken', 'publicNameAgreeTerms'],
  'developer/carlink': ['physicalWidthEnabled', 'physicalWidthOverrideMm', 'carlinkHevc', 'carlinkTrace', 'carlinkLink', 'carlinkAddress', 'carlinkHeadUnit', 'carlinkLocal', 'carlinkPictureSize'],
  'developer/video': ['webTranscode', 'displayMode', 'displayWidth', 'displayHeight', 'displayDensity', 'fps'],
  'developer/connection': ['carlinkModel', 'receiverPort', 'rfcommChannel', 'hotspotChannel', 'hotspotSecurity', 'wifiChannel', 'devicePreset', 'playCarPlayAudio', 'microphoneSourceEnabled', 'microphoneSource'],
  'identity': ['identity', 'identitySource', 'tlsCertificate', 'mfiTarget', 'mfiI2cPath', 'mfiRemoteServer', 'mfiRemoteToken'],
  developer: ['developerActive'],
  'experimental': [...Object.values(experimentalReleases).flat(), 'experimentalActive'],
};
export function fieldEnabled(key: string, values: Settings['values']): boolean {
  if (key === 'microphoneSource' && values.microphoneSourceEnabled === false) return false;
  if (key.startsWith('experimental') && key !== 'experimentalActive') return values.experimentalActive !== false;
  return !['physicalWidthEnabled', 'physicalWidthOverrideMm', 'webTranscode', 'playCarPlayAudio', 'microphoneSourceEnabled', 'microphoneSource', 'carlinkTrace', 'carlinkHevc', 'carlinkPictureSize', 'carlinkModel', 'receiverPort', 'rfcommChannel', 'devicePreset', 'carlinkLink', 'carlinkAddress', 'carlinkHeadUnit', 'carlinkLocal'].includes(key) || values.developerActive !== false;
}
export function fieldRoute(key: string): string {
  return 'settings/' + (Object.entries(pageKeys).find(([, keys]) => keys.includes(key))?.[0] ?? 'developer/other');
}
export function pageFields(settings: Settings, route: string): Field[] { return settings.schema.flatMap(s => s.fields).filter(f => fieldRoute(f.key) === route); }
export const pages = [
  ['output', 'menu.output', 'cast'], ['car', 'menu.car', 'car'], ['tesla', 'menu.tesla', 'globe'], ['picture', 'menu.picture', 'monitor'], ['audio', 'menu.audio', 'bluetooth'],
  ['connection', 'menu.connection', 'smartphone'], ['startup', 'menu.startup', 'play'], ['access', 'menu.access', 'key-round'],
  ['identity', 'menu.identity', 'file-key-2'], ['config-files', 'menu.config-files', 'files'], ['logs', 'menu.logs', 'file-text'], ['about', 'menu.about', 'info'],
  ['developer', 'menu.developer', 'code'], ['experimental', 'menu.experimental', 'flask-conical'],
] as const;
export const accessIcons = { publicName: 'shield-check', admin: 'lock' } as const;
export const developerPages = ['diagnostics', 'carlink', 'video', 'connection', 'other'] as const;
export function routeTitle(route: string): string { return route.startsWith('setup') ? 'setup.title' : route === 'settings' ? 'ui.settings' : route.startsWith('settings/developer/') ? `menu.developer.${route.split('/').at(-1)}` : `menu.${route.slice(9)}`; }
export function parentRoute(route: string): string { return route.includes('/') ? route.slice(0, route.lastIndexOf('/')) : 'home'; }
export function normalizeRoute(hash: string): string {
  let route = hash.replace(/^#\/?/, '') || 'home';
  if (['settings/developer/identity', 'developer/identity', 'settings:identity'].includes(route)) route = 'settings/identity';
  const valid = ['home', 'setup', ...['car', 'output', 'connection', 'network', 'prepare', 'start'].map(step => 'setup/' + step), 'settings', ...pages.map(([p]) => `settings/${p}`), ...Object.keys(pageKeys).map(p => `settings/${p}`), ...developerPages.map(p => `settings/developer/${p}`), 'settings/access/admin'];
  return valid.includes(route) ? route : 'settings';
}
export function settingsPlacement(settings: Settings) { return settings.schema.flatMap(s => s.fields).map(field => ({ field, route: fieldRoute(field.key) })); }
export function restartsPicture(key: string): boolean { return ['carplayResolution', 'carplayScreenWidth', 'resolution', 'screenWidth', 'videoCodec', 'frameRate'].includes(key); }

export const developerIcons: Record<string, string> = { diagnostics: 'info', carlink: 'car', video: 'settings', connection: 'wifi', identity: 'file-key-2', other: 'sliders-horizontal' };
export function pageAvailable(settings: Settings, status: Status, route: string): boolean {
  if (route.startsWith('setup')) return true;
  if (route === 'settings/logs' || route === 'settings/config-files') return status.adminFeatures?.includes(route.slice(9) as 'logs' | 'config-files') ?? false;
  if (['settings', 'settings/about', 'settings/access', 'settings/access/admin', 'settings/connection', 'settings/connection/network', 'settings/developer', 'settings/developer/diagnostics', 'settings/developer/connection'].includes(route)) return true;
  if (route === 'settings/identity') return status.adminFeatures?.includes('identity') ?? pageFields(settings, route).length > 0;
  if (route === 'settings/vehicle') return status.platform === 'linux';
  if (route === 'settings/car') return status.platform === 'linux' || pageFields(settings, route).length > 0;
  if (route === 'settings/connection/network/interfaces') return status.platform === 'linux';
  if (route === 'settings/tesla' && status.platform === 'linux' && settings.values.carBrand !== 'tesla' && (!settings.values.teslaBrowser || settings.values.teslaBrowser === 'off')) return false;
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
    if (!field || !isVisible(field, v) || value === '' || value == null) return '';
    return field.type === 'choice' ? optionLabel(field.options?.find(o => o.value === value), language, t) : field.type === 'bool' ? t(value ? 'stage.on' : 'stage.off') : typeof value === 'string' || typeof value === 'number' ? String(value) : '';
  };
  if (route === 'settings/car') return ['carBrand', 'carModel', 'carYear'].map(key => v[key] === 'other' && v[key + 'Text'] ? String(v[key + 'Text']) : label(key)).filter(Boolean).join(' · ');
  if (route === 'settings/picture') {
    const custom = (key: string) => v[key] === 'auto' ? '' : label(key);
    const widthKey = all.some(field => field.key === 'screenWidth') ? 'screenWidth' : 'carplayScreenWidth';
    const width = v[widthKey];
    return [custom('resolution') || custom('carplayResolution'), width !== 200 && label(widthKey) ? `${width} mm` : '', custom('videoCodec'), custom('frameRate')].filter(Boolean).join(' · ');
  }
  if (route === 'settings/tesla') return label('teslaBrowser');
  if (route === 'settings/output') return label('output');
  if (route === 'settings/audio') return v.carBluetoothAudio === false && label('carBluetoothAudio') ? `${t('stage.carBluetooth')}: ${label('carBluetoothAudio')}` : '';
  if (route === 'settings/startup') return [['autoStart', 'menu.whenReady'], ['bootStart', 'menu.atBoot']].map(([key, title]) => v[key] === false && label(key) ? `${t(title)}: ${label(key)}` : '').filter(Boolean).join(' · ');
  if (route === 'settings/connection') return v.transport === 'usb' ? label('transport') : '';
  if (route === 'settings/connection/network') return label('networkMode');
  if (route === 'settings/access') return [status.adminEnabled === false ? `${t('menu.admin')}: ${t('stage.off')}` : '', v.publicNameEnabled === true && label('publicNameEnabled') ? `${t('publicName.title')}: ${t('stage.on')}` : ''].filter(Boolean).join(' · ');
  if (route === 'settings/experimental') {
    if (!v.experimentalActive) return t('stage.off');
    const options = all.filter(f => f.key.startsWith('experimental') && f.key !== 'experimentalActive' && f.type === 'bool' && v[f.key]).length;
    return options ? `${t('stage.on')} · ${options} ${t('menu.options')}` : t('stage.on');
  }
  if (route === 'settings/identity' && v.mfiTarget && v.mfiTarget !== 'local') return mfiReadiness(v, t);
  if (route === 'settings/identity') return t(status.identity?.mfi.state === 'READY' ? 'direlay_installed' : status.identity?.mfi.state === 'INVALID' ? 'dev_invalid_identity' : 'direlay_missing');
  if (route === 'settings/about') return updateMenuVersion(status.version, t);
  if (route === 'settings/developer/carlink') return [v.carlinkHevc !== 'auto' ? label('carlinkHevc') : '', v.carlinkTrace !== 'off' ? label('carlinkTrace') : ''].filter(Boolean).join(' · ');
  if (route === 'settings/developer/connection') return [label('output'), t(v.transport === 'usb' ? 'option.transport.usb' : 'option.transport.wireless')].filter(Boolean).join(' · ');
  if (route === 'settings/developer/video') return ['custom', 'fixed'].includes(String(v.displayMode)) ? `${v.displayWidth} × ${v.displayHeight} · ${v.displayDensity} dpi` : v.displayMode === 'auto' ? '' : label('displayMode');
  return '';
}
