// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-27

import { normalizeInput, sections, sectionView, stepInfo, type PartialInput, type PathInput, type PathStep, type StepId, type StepState, type StepText, type StepAction, type LinkStep, type LinkFailure, type ConnectionPath } from './model';
import { titles, explanations, actionLabels, failureExplanation, linuxFailureExplanation, failureHeadline } from './texts';
const text = (res: string, ...args: string[]): StepText => ({ res, args });
const raw = (value: string | null | undefined): StepText | null => value == null ? null : { raw: value };
const has = <T>(values: readonly T[], value: T) => values.includes(value);
const wirelessIds: StepId[] = ['IPHONE_PAIRED', 'CARPLAY_START', 'IPHONE_BLUETOOTH', 'IPHONE_AUTH', 'IPHONE_WIFI', 'SESSION'];
const wiredIds: StepId[] = ['USB_HOST', 'CARPLAY_START', 'USB_IPHONE', 'USB_TRUST', 'USB_CONTROL', 'SESSION'];
const carlinkIds: StepId[] = ['CARLINK_FIND', 'CARLINK_LINK', 'CARLINK_PIN', 'CARLINK_READY'];
function step(id: StepId, state: StepState = 'BLOCKED', detail: StepText | null = null, evidence: StepText | null = null, code: string | null = null, action: StepAction | null = null, sinceMillis: number | null = null, trail: string[] = []): PathStep {
  return { id, state, title: titles[id], detail: detail ?? (state === 'BLOCKED' ? text('path_after_step') : null), evidence, code, action, actionLabel: action ? actionLabels[action] : null, sinceMillis, explanation: failureExplanation(code) ?? explanations[id] ?? null, trail: [...trail], carWaitUntilMillis: null };
}
export function stepIds(input: PathInput): StepId[] {
  const { setup: s, relay: r } = input;
  const out: StepId[] = ['MODE', 'IDENTITY'];
  if (input.platform !== 'linux') out.push('PERMISSIONS');
  if (has(['PENDING', 'REFUSED'], r.keepAlive)) out.push('KEEP_ALIVE');
  if (s.network !== 'USB' || s.outputs.includes('CARLINK')) out.push('BLUETOOTH');
  if (has(['WIFI_DIRECT', 'SHARED_WIFI'], s.network) || s.outputs.some(o => o === 'CARLINK' || o === 'MIRACAST')) out.push('WIFI');
  if (s.network === 'PHONE_HOTSPOT') out.push('HOTSPOT');
  if (s.network === 'SHARED_WIFI') out.push('SHARED_WIFI');
  if (s.outputs.some(o => o === 'BROWSER' || o === 'DLNA')) out.push('CAR_NETWORK');
  if (s.outputs.includes('BROWSER')) out.push('WEB_SERVER', 'CAR_PAGE');
  if (s.outputs.includes('CARLINK')) out.push(...carlinkIds);
  if (s.outputs.includes('DLNA')) out.push('DLNA');
  if (s.outputs.includes('MIRACAST')) out.push('MIRACAST');
  out.push(...(s.network === 'USB' ? wiredIds : wirelessIds));
  if (s.outputs.includes('TESLA')) out.push('TESLA');
  return [...out, 'PICTURE'];
}
function prerequisites(ids: StepId[], input: PathInput): Partial<Record<StepId, StepId[]>> {
  const iphone = ids.filter(id => stepInfo[id].section === 'IPHONE');
  const p: Partial<Record<StepId, StepId[]>> = {
    IDENTITY: ['MODE'], PERMISSIONS: ['MODE'], CAR_NETWORK: input.setup.network === 'PHONE_HOTSPOT' ? ['HOTSPOT'] : input.setup.network === 'SHARED_WIFI' ? ['SHARED_WIFI'] : [],
    CAR_PAGE: ['CAR_NETWORK', 'WEB_SERVER'], CARLINK_FIND: ['BLUETOOTH', 'WIFI'], CARLINK_LINK: ['CARLINK_FIND'], CARLINK_PIN: ['CARLINK_LINK'], CARLINK_READY: ['CARLINK_PIN'], DLNA: ['CAR_NETWORK'], MIRACAST: ['WIFI'], IPHONE_PAIRED: ['BLUETOOTH'], TESLA: ['SESSION'], PICTURE: ['SESSION'],
  };
  for (const id of ['BLUETOOTH', 'WIFI', 'HOTSPOT', 'SHARED_WIFI'] as StepId[]) p[id] = ids.includes('PERMISSIONS') ? ['PERMISSIONS'] : [];
  iphone.slice(1).forEach((id, index) => { p[id] = [iphone[index]]; });
  p.CARPLAY_START = [...ids.filter(id => stepInfo[id].section === 'RELAY'), iphone[0]];
  return p;
}
function linkStepId(s: LinkStep, wireless: boolean): StepId {
  switch (s) {
    case 'IDLE': case 'STARTING': return 'CARPLAY_START';
    case 'WAITING_IPHONE': case 'BLUETOOTH': case 'USB_DETECT': case 'USB_PERMISSION': return wireless ? 'IPHONE_BLUETOOTH' : 'USB_IPHONE';
    case 'AUTH': case 'USB_CONTROL': return wireless ? 'IPHONE_AUTH' : 'USB_CONTROL';
    case 'WIFI_SENT': case 'JOINING': return wireless ? 'IPHONE_WIFI' : 'USB_CONTROL';
    case 'USB_TRUST': return wireless ? 'IPHONE_AUTH' : 'USB_TRUST';
    default: return 'SESSION';
  }
}
function failureStep(f: LinkFailure, wireless: boolean): StepId {
  if (f.code.startsWith('hotspot_') || has(['wifi_off', 'wifi_reset_required'], f.code)) return 'CARPLAY_START';
  if (wireless && has(['bluetooth_off', 'iphone_unpaired', 'rfcomm_timeout', 'bluetooth_unusable'], f.code)) return 'IPHONE_BLUETOOTH';
  if (wireless && f.code === 'first_tcp_timeout') return 'IPHONE_WIFI';
  if (!wireless && has(['usb_trust_denied', 'iphone_locked'], f.code)) return 'USB_TRUST';
  if (!wireless && f.code === 'usb_role') return 'USB_HOST';
  return linkStepId(f.step, wireless);
}
function presentStep(id: StepId, input: PathInput, canStart: boolean, running: boolean): PathStep {
  const { setup, relay: r, car: c, iphone: i, live: l } = input;
  const wireless = setup.network !== 'USB';
  const link = c.carlink;
  const failedLink = link.code ? link.code === 'carlink_blocked' || link.code.startsWith('ble_') || link.code.startsWith('gatt_') ? 'CARLINK_FIND' : link.code === 'projection_timeout' ? 'CARLINK_READY' : 'CARLINK_LINK' : null;
  const currentLink = failedLink ?? (has(['OFF', 'UNSUPPORTED', 'NEEDS_PERMISSIONS', 'BLOCKED', 'SCANNING'], link.phase) ? 'CARLINK_FIND' : has(['EXCHANGING', 'JOINING', 'AUTHENTICATING'], link.phase) ? 'CARLINK_LINK' : link.phase === 'PIN' ? 'CARLINK_PIN' : 'CARLINK_READY');
  const passed = (id: StepId) => carlinkIds.indexOf(id) < carlinkIds.indexOf(currentLink);
  const clink = (state: StepState, detail: StepText | null = null, evidence: StepText | null = null, action: StepAction | null = null) => {
    const failed = failedLink === id;
    return step(id, failed ? link.retrying ? 'WORKING' : 'FAILED' : state,
      failed && link.retrying ? text(input.platform === 'linux' ? 'path_trying_again' : 'path_retrying', link.code!) : failed && !failureExplanation(link.code) && link.error != null ? raw(link.error) : detail,
      evidence, failed ? link.code : null, failed ? link.code === 'carlink_blocked' ? 'FIX_SETTINGS' : 'RECONNECT_CARLINK' : action, link.sinceMillis, link.trail);
  };
  const progress = (state: StepState, detail: StepText | null = null, evidence: StepText | null = null) => {
    const order = (wireless ? wirelessIds : wiredIds).slice(1);
    const current = !i.started && has(['IDLE', 'ENDED'], i.step) ? -1 : order.indexOf(linkStepId(i.step, wireless));
    const failed = i.failure ? order.indexOf(failureStep(i.failure, wireless)) : -1;
    const at = Math.max(current, failed), own = order.indexOf(id);
    return own < at ? step(id, 'OK') : own === at ? step(id, state, detail, evidence, null, null, i.stepSinceMillis) : step(id);
  };
  let own: PathStep;
  switch (id) {
    case 'MODE': own = setup.valid ? step(id, 'OK', null, raw(setup.summary)) : step(id, 'ACTION', text('path_settings_missing'), null, null, 'FIX_SETTINGS'); break;
    case 'IDENTITY': own = r.identity === 'READY' ? step(id, 'OK', null, raw(r.identityLabel)) : r.identity === 'CHECKING' ? step(id, 'WORKING', text('path_checking')) : step(id, 'ACTION', text('path_import_identity'), null, null, 'IMPORT_IDENTITY'); break;
    case 'PERMISSIONS': {
      const total = r.permissionsTotal + Number(r.vpnConsentNeeded) + Number(r.overlayNeeded);
      const missing = Math.max(0, r.permissionsTotal - r.permissionsGranted) + Number(r.vpnConsentNeeded) + Number(r.overlayNeeded);
      own = missing === 0 ? step(id, 'OK', null, raw(`${r.permissionsGranted}/${total}`)) : step(id, 'ACTION', r.overlayNeeded && missing === 1 ? text('path_allow_overlay') : r.vpnConsentNeeded && missing === 1 ? text('path_allow_vpn') : text('path_permissions_needed', String(missing)), null, null, 'GRANT_PERMISSIONS'); break;
    }
    case 'KEEP_ALIVE': own = r.keepAlive === 'PENDING' ? step(id, 'WORKING', text('path_keep_screen_open')) : step(id, 'ACTION', text('path_background_stopped'), null, null, 'KEEP_ALIVE_SETTINGS'); break;
    case 'BLUETOOTH': case 'WIFI': case 'HOTSPOT': {
      const on = id === 'BLUETOOTH' ? r.bluetoothOn : id === 'WIFI' ? r.wifiOn : r.hotspotOn;
      const evidence = id === 'BLUETOOTH' ? raw(r.bluetoothName) ?? text('path_on') : id === 'WIFI' ? raw(r.wifiSsid) ?? text('path_on') : raw([r.hotspotSsid, r.hotspotBand].filter(v => v != null).join(' · '));
      own = on === true ? step(id, 'OK', null, evidence) : on === false ? step(id, 'ACTION', id === 'HOTSPOT' ? text('path_turn_on_hotspot', r.hotspotSsid ?? '') : text(id === 'WIFI' ? 'path_turn_on_wifi' : 'path_turn_on_bluetooth'), null, null, id === 'HOTSPOT' ? 'OPEN_HOTSPOT' : id === 'WIFI' ? 'OPEN_WIFI' : 'TURN_ON_BLUETOOTH') : step(id, 'WAITING', text('path_radio_unknown')); break;
    }
    case 'SHARED_WIFI': own = r.sharedWifiSsid != null ? step(id, 'OK', null, raw(r.sharedWifiSsid)) : step(id, 'ACTION', text('path_join_shared_wifi'), null, null, 'CHOOSE_SHARED_WIFI'); break;
    case 'CAR_NETWORK': own = c.address != null ? step(id, 'OK', null, raw(c.address)) : step(id, 'WAITING', text('path_car_network_wait')); break;
    case 'WEB_SERVER': own = c.web.phase === 'LISTENING' ? step(id, 'OK', null, raw(c.web.trustedName ?? (c.web.port != null ? `:${c.web.port} HTTPS` : null))) : c.web.phase === 'FAILED' ? step(id, 'FAILED', c.web.code === 'web_port_busy' ? text('path_port_busy', String(c.web.port ?? '')) : text('path_could_not_start'), null, c.web.code ?? 'web_failed', null, c.web.sinceMillis) : step(id, 'WORKING', text('path_starting')); break;
    case 'CAR_PAGE': own = c.pages > 0 ? step(id, 'OK', text('path_connected'), raw(c.screen)) : step(id, 'WAITING', text('path_open_car_page'), raw(c.pageUrl), null, 'COPY_PAGE_ADDRESS'); break;
    case 'CARLINK_FIND':
      own = link.phase === 'UNSUPPORTED' ? step(id, 'FAILED', text('path_carlink_unsupported'), null, null, null, link.sinceMillis)
        : link.phase === 'BLOCKED' ? { ...clink(link.retrying ? 'WORKING' : 'FAILED', link.retrying ? text(input.platform === 'linux' ? 'path_trying_again' : 'path_retrying', 'carlink_blocked') : text('path_carlink_blocked'), null, 'FIX_SETTINGS'), code: 'carlink_blocked', explanation: 'path_explain_carlink_blocked' }
        : link.phase === 'NEEDS_PERMISSIONS' ? step(id, 'BLOCKED', text('path_after_step'))
        : failedLink === id ? clink('FAILED', text('path_car_not_found')) : passed(id) ? clink('OK', null, raw(link.carName)) : clink('WAITING', text('path_open_carlink')); break;
    case 'CARLINK_LINK': {
      const detail = ({ EXCHANGING: 'path_exchanging', JOINING: 'path_joining_car_wifi', AUTHENTICATING: 'path_verifying' } as Record<string, string>)[link.phase];
      own = failedLink === id ? clink('FAILED', text('path_link_failed')) : passed(id) ? clink('OK', null, raw(link.joinMethod)) : detail ? clink('WORKING', text(detail)) : clink('BLOCKED'); break;
    }
    case 'CARLINK_PIN': own = link.pinPending ? { ...clink('ACTION', text(input.platform === 'linux' && link.pinConfirmation ? 'path_approve_car' : 'path_enter_pin'), null, 'ENTER_PIN'), actionLabel: input.platform === 'linux' && link.pinConfirmation ? 'path_action_approve' : actionLabels.ENTER_PIN } : link.trusted || passed(id) ? clink('OK', text('path_trusted')) : clink('BLOCKED'); break;
    case 'CARLINK_READY': own = failedLink === id ? clink('FAILED', raw(link.code)) : link.phase === 'NEGOTIATING' ? clink('WORKING', text('path_negotiating')) : link.phase === 'PLAYING' ? clink('OK', null, raw(link.picture)) : link.phase === 'ENDED' ? clink('WAITING', text('path_carlink_disconnected'), null, 'RECONNECT_CARLINK') : clink('BLOCKED'); break;
    case 'DLNA': {
      const o = c.dlna;
      own = o.phase === 'SEARCHING' ? step(id, 'WORKING', text('path_searching'), null, null, null, o.sinceMillis) : has(['OFF', 'WAITING_USER'], o.phase) ? step(id, 'ACTION', text('path_choose_receiver'), null, null, 'CHOOSE_RENDERER') : o.phase === 'FAILED' ? step(id, 'FAILED', raw(o.code), null, o.code, 'CHOOSE_RENDERER', o.sinceMillis) : step(id, 'OK', null, raw(o.name)); break;
    }
    case 'MIRACAST': own = c.miracast.phase === 'PLAYING' ? step(id, 'OK', null, raw(c.miracast.name)) : c.miracast.phase === 'FAILED' ? step(id, 'FAILED', raw(c.miracast.code), null, c.miracast.code, null, c.miracast.sinceMillis) : step(id, 'WAITING', text('path_cast_to_car'), null, null, 'OPEN_CAST', c.miracast.sinceMillis); break;
    case 'IPHONE_PAIRED': own = i.pick === 'PICKED' ? step(id, 'OK', null, raw(i.name)) : step(id, 'ACTION', i.pick === 'NONE' ? text('path_pair_iphone', r.bluetoothName ?? '') : i.pick === 'NOT_IPHONE' ? text('path_not_iphone', i.name ?? '') : text(i.pick === 'UNPAIRED' ? 'path_iphone_unpaired' : 'path_choose_iphone'), null, null, i.pick === 'NONE' ? 'PAIR_IPHONE' : 'CHOOSE_IPHONE'); break;
    case 'USB_HOST': own = i.usb === 'HOST' ? step(id, 'OK', text('path_usb_host_mode')) : i.usb === 'DEVICE' ? step(id, 'FAILED', text('path_usb_device_mode'), null, 'usb_role') : step(id, 'WAITING', text('path_plug_iphone')); break;
    case 'CARPLAY_START': own = i.carWaitUntilMillis != null ? { ...step(id, 'WAITING', text('path_car_screen_wait'), null, null, 'START_NOW', i.stepSinceMillis), carWaitUntilMillis: i.carWaitUntilMillis } : i.armed ? step(id, 'BLOCKED', text('path_start_armed')) : i.step === 'IDLE' && !i.started && !i.failure ? step(id, 'BLOCKED', canStart ? text('path_tap_connect') : null) : progress('WORKING', text('path_preparing')); break;
    case 'IPHONE_BLUETOOTH': own = progress(i.step === 'WAITING_IPHONE' ? 'WAITING' : 'WORKING', i.step === 'WAITING_IPHONE' ? text(input.platform === 'linux' ? 'state.waiting' : 'path_iphone_near') : text('path_connecting_iphone', i.name ?? '')); break;
    case 'IPHONE_AUTH': own = progress('WORKING', input.platform === 'linux' ? text('path_checking_iphone') : null, raw(i.stepDetail)); break;
    case 'IPHONE_WIFI': own = progress('WORKING', setup.network === 'PHONE_HOTSPOT' ? text('path_iphone_joining_wifi', r.hotspotSsid ?? '') : setup.network === 'SHARED_WIFI' ? r.sharedWifiSsid != null ? text('path_iphone_joining_wifi', r.sharedWifiSsid) : text('path_iphone_joining_car_hotspot') : text('path_iphone_joining_wifi', 'Wi-Fi Direct')); break;
    case 'USB_IPHONE': own = progress(i.step === 'USB_PERMISSION' ? 'ACTION' : 'WAITING', text(i.step === 'USB_PERMISSION' ? 'path_allow_usb' : 'path_plug_unlock_iphone')); break;
    case 'USB_TRUST': own = progress('WAITING', text('path_trust_iphone')); break;
    case 'USB_CONTROL': own = progress('WORKING', text('path_connecting')); break;
    case 'SESSION': own = i.step === 'VIDEO' ? step(id, 'OK', null, raw(l.chips.slice(0, 2).join(' · '))) : i.step === 'ENDED' && running ? step(id, 'WAITING', text('path_reconnecting'), null, null, null, i.stepSinceMillis) : progress('WORKING', text('path_waiting_picture')); break;
    case 'TESLA': own = i.step !== 'VIDEO' ? step(id, 'BLOCKED', text('path_tesla_after_session')) : c.tesla.phase === 'FAILED' ? step(id, 'FAILED', raw(c.tesla.code), null, c.tesla.code, null, c.tesla.sinceMillis) : has(['READY', 'PLAYING'], c.tesla.phase) ? step(id, 'OK', null, raw(c.tesla.name)) : step(id, 'BLOCKED', text('path_tesla_after_session')); break;
    case 'PICTURE': own = i.step !== 'VIDEO' ? step(id, 'BLOCKED', text('path_picture_after_session')) : l.pictureOnCar ? step(id, 'OK', null, raw(l.chips.join(' · '))) : step(id, 'WAITING', text('path_waiting_car_picture'), null, null, null, l.sessionSinceMillis); break;
  }
  const f = i.failure;
  if (!f || failureStep(f, wireless) !== id) return own;
  const action = id === 'CARPLAY_START' && f.code.startsWith('hotspot_') && r.hotspotOn === false ? 'OPEN_HOTSPOT' : 'RETRY';
  return step(id, f.retrying ? 'WORKING' : 'FAILED', f.retrying ? text(input.platform === 'linux' ? 'path_trying_again' : 'path_retrying', f.code) : f.code === 'first_tcp_timeout' ? text('path_iphone_did_not_join') : raw(f.text), own.evidence, f.code, action, f.atMillis, f.trail);
}
export function clockTime(millis: number, zone?: string): string {
  return new Intl.DateTimeFormat('en-GB', { hour: '2-digit', minute: '2-digit', second: '2-digit', hourCycle: 'h23', ...(zone ? { timeZone: zone } : {}) }).format(millis);
}
/** Pure Android presenter port. Only waiting/action steps inherit prerequisite blocking. */
export function presentPath(partial: PartialInput, zone?: string): ConnectionPath {
  const input = normalizeInput(partial), i = input.iphone, r = input.relay;
  const running = i.started || i.armed || !has(['IDLE', 'ENDED'], i.step);
  const canStart = input.setup.valid && r.identity === 'READY' && r.permissionsGranted >= r.permissionsTotal && !r.vpnConsentNeeded && !r.overlayNeeded && (input.setup.network === 'USB' || i.pick === 'PICKED') && !running;
  const ids = stepIds(input), p = prerequisites(ids, input), checked = new Map<StepId, PathStep>();
  const unreadable = (s: PathStep) => s.state === 'WAITING' && (s.id === 'BLUETOOTH' && r.bluetoothOn == null || s.id === 'WIFI' && r.wifiOn == null || s.id === 'HOTSPOT' && r.hotspotOn == null);
  for (const id of ids) {
    let own = presentStep(id, input, canStart, running);
    if (input.platform === 'linux' && own.code) {
      const explanation = linuxFailureExplanation(own.code);
      if (own.state === 'FAILED') own = { ...own, detail: text(failureHeadline(own.code)), explanation };
      else if (own.state === 'WORKING') own = { ...own, detail: text('path_trying_again'), explanation };
    }
    const blocked = (p[id] ?? []).some(id => { const previous = checked.get(id); return previous && previous.state !== 'OK' && !unreadable(previous); });
    checked.set(id, blocked && has(['WAITING', 'ACTION', 'BLOCKED'], own.state) ? { ...own, state: 'BLOCKED', detail: own.detail ?? text('path_after_step') } : own);
  }
  const steps = [...checked.values()];
  const views = sections.map(section => sectionView(section, steps.filter(s => stepInfo[s.id].section === section))).filter(s => s.total);
  const live = input.live.pictureOnCar && has(['SESSION', 'VIDEO'], i.step);
  const tone = live ? 'LIVE' : steps.some(s => s.state === 'FAILED') ? 'ERROR' : steps.some(s => s.state === 'ACTION') ? 'ACTION' : running || steps.some(s => s.state === 'WORKING') ? 'PROGRESS' : canStart ? 'READY' : 'IDLE';
  const focus = steps.find(s => s.state === 'FAILED' || s.state === 'ACTION') ?? steps.find(s => s.state === 'WORKING' || s.state === 'WAITING') ?? steps.find(s => s.state === 'BLOCKED') ?? null;
  const glyph = { OK: '✓', WORKING: '~', WAITING: '~', ACTION: '!', FAILED: '✗', BLOCKED: '·' };
  const parts = views.map(s => `${s.section[0]} ${s.steps.map(s => stepInfo[s.id].code + glyph[s.state]).join(' ')}`);
  const failure = steps.find(s => s.state === 'FAILED' && s.code != null);
  if (failure) parts.push(`E ${failure.code} ${clockTime(failure.sinceMillis ?? 0, zone)}`);
  return { ...(input.platform === 'linux' ? { platform: 'linux' as const, armed: i.armed } : {}), sections: views, focus, tone, canStart, running, live: live ? { sinceMillis: input.live.sessionSinceMillis, chips: [...input.live.chips] } : null,
    nodes: { iphone: i.name, relay: r.bluetoothName, car: input.car.carlink.carName ?? input.car.screen ?? (input.setup.outputs.includes('BROWSER') ? input.car.address : null) ?? input.car.dlna.name ?? input.car.miracast.name }, diagnostic: parts.join(' | '), version: input.version };
}
