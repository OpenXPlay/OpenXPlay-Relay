// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-16

import type { Status } from './api';
import { icon } from './icons';
type Translate = (key: string, parameters?: Record<string, string | number>) => string;
const esc = (s: unknown) => String(s ?? '').replace(/[&<>"']/g, c => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' })[c]!);

// Re-anchor the relay's deadline on this browser's clock: head units often have the wrong time.
// A drift of one second between polls keeps the previous deadline so the countdown never jumps back.
export function anchorPairing(status: Status, now = Date.now() / 1000, previous: number | null = null): Status {
  if (typeof status.pairingRemaining !== 'number') return status;
  if (status.pairingRemaining <= 0) return { ...status, pairingDeadline: null };
  const deadline = Math.round(now + status.pairingRemaining);
  return { ...status, pairingDeadline: previous !== null && Math.abs(previous - deadline) <= 1 ? previous : deadline };
}
export function formatRemaining(seconds: number): string {
  const total = Math.max(0, Math.ceil(seconds));
  return `${Math.floor(total / 60)}:${String(total % 60).padStart(2, '0')}`;
}
export function pairingView(status: Status, now = Date.now() / 1000) {
  const seconds = Math.max(0, Math.ceil((status.pairingDeadline ?? 0) - now));
  const devices = [...(status.pairedDevices ?? [])].sort((a, b) => Number(b.connected) - Number(a.connected) || Number(b.iphone) - Number(a.iphone) || a.name.localeCompare(b.name));
  return { available: status.pairingAvailable === true, open: seconds > 0, seconds, time: formatRemaining(seconds),
    cancelable: seconds > 0 && status.pairingCancelable === true, confirm: seconds === 0 && status.pairingNeedsConfirmation === true,
    // Android 12+ asks for Nearby devices first; in the background the request waits in a notification.
    permission: seconds === 0 && status.pairingNeedsPermission === true, notified: seconds === 0 && status.pairingNotified === true,
    // Android without the dialog permission: its Bluetooth settings keep the relay visible while open.
    onRelay: seconds === 0 && status.pairingOnRelay === true, denied: seconds === 0 && status.pairingOnRelay === true && status.pairingPermissionDenied === true, devices };
}
export function pairingPanelView(status: Status, busy: boolean, error: string, t: Translate, now = Date.now() / 1000): string {
  const v = pairingView(status, now);
  const control = v.open
    ? `<p id="pairing-remaining" role="timer">${esc(t('pairing.open', { time: v.time }))}</p><p>${t('pairing.howTo')}</p>${v.cancelable ? `<button type="button" id="pairing-cancel" ${busy ? 'disabled' : ''}>${t('pairing.cancel')}</button>` : `<p class="muted">${t('pairing.noCancel')}</p>`}`
    : v.notified ? `<p role="status">${t('pairing.notified')}</p>`
    : v.permission ? `<p role="status">${t('pairing.permission')}</p>`
    : v.confirm ? `<p role="status">${t('pairing.confirm')}</p>`
    : v.onRelay ? `<p role="status">${t('pairing.settingsOpen')}</p><p>${t('pairing.howTo')}</p>${v.denied ? `<p class="muted">${t('pairing.permissionDenied')}</p>` : ''}`
    : v.available ? `<button type="button" id="pairing-open" ${busy ? 'disabled' : ''}>${t('pairing.start')}</button>` : `<p>${t('home.pairOnDevice')}</p>`;
  const rows = v.devices.map(d => `<li>${icon(d.iphone ? 'smartphone' : 'bluetooth')}<span>${esc(d.name || t('pairing.unnamed'))}</span>${d.connected ? `<span class="tag">${t('ui.connected')}</span>` : ''}</li>`).join('');
  return `<div class="field pairing"><h3>${t('pairing.title')}</h3><p class="muted">${t('pairing.when')}</p>${control}<p id="pairing-result" class="error" role="alert">${esc(error ? t(error) : '')}</p><h4>${t('pairing.devices')}</h4><ul class="paired-devices">${rows || `<li class="muted">${t('pairing.none')}</li>`}</ul></div>`;
}

// One second ticker for every visible pairing countdown; onExpire runs once when an open window reaches zero.
export function createCountdown(deadline: () => number | null | undefined, render: (seconds: number) => void, onExpire: () => void, clock = () => Date.now() / 1000) {
  let timer: ReturnType<typeof setInterval> | undefined;
  let open = false;
  const step = () => {
    const seconds = Math.max(0, Math.ceil((deadline() ?? 0) - clock()));
    render(seconds);
    if (seconds > 0) open = true;
    else if (open) { open = false; onExpire(); }
  };
  return {
    start() { if (timer === undefined) { step(); timer = setInterval(step, 1000); } },
    stop() { clearInterval(timer); timer = undefined; },
  };
}
