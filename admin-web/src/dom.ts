// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-27

import { esc } from './html';

const listeners = new WeakMap<EventTarget, Map<string, EventListener>>();
/** Rebinding a patched row replaces its handler without replacing the row. */
export function on<K extends keyof GlobalEventHandlersEventMap>(target: EventTarget | null | undefined, type: K, listener: (event: GlobalEventHandlersEventMap[K]) => void): void;
export function on(target: EventTarget | null | undefined, type: string, listener: (event: Event) => void): void;
export function on(target: EventTarget | null | undefined, type: string, listener: (event: any) => void) {
  if (!target) return;
  const events = listeners.get(target) ?? new Map<string, EventListener>();
  const previous = events.get(type); if (previous) target.removeEventListener(type, previous);
  events.set(type, listener as EventListener); listeners.set(target, events); target.addEventListener(type, listener as EventListener);
}
const key = (node: Node) => node instanceof Element ? node.id || node.getAttribute('data-key') || node.getAttribute('data-field') || node.getAttribute('data-step') || node.getAttribute('data-section') : null;
function morph(current: Node, next: Node): Node {
  if (current.nodeType !== next.nodeType || current.nodeName !== next.nodeName) { current.parentNode!.replaceChild(next, current); return next; }
  if (!(current instanceof Element) || !(next instanceof Element)) { if (current.nodeValue !== next.nodeValue) current.nodeValue = next.nodeValue; return current; }
  for (const attr of [...current.attributes]) if (!next.hasAttribute(attr.name) && !(current instanceof HTMLDetailsElement && attr.name === 'open')) current.removeAttribute(attr.name);
  for (const attr of [...next.attributes]) if (current.getAttribute(attr.name) !== attr.value && !(current instanceof HTMLDetailsElement && attr.name === 'open')) current.setAttribute(attr.name, attr.value);
  if (!next.hasAttribute('data-preserve')) patchChildren(current, next);
  return current;
}
function patchChildren(current: Element | DocumentFragment, next: Element | DocumentFragment) {
  const keyed = new Map([...current.childNodes].filter(n => key(n)).map(n => [key(n), n]));
  let cursor = current.firstChild;
  for (const wanted of [...next.childNodes]) {
    const wantedKey = key(wanted);
    const match = wantedKey ? keyed.get(wantedKey) : cursor && !key(cursor) ? cursor : null;
    let node: Node;
    if (match) node = morph(match, wanted);
    else node = wanted;
    if (!match || match !== cursor) current.insertBefore(node, cursor);
    cursor = node.nextSibling;
  }
  while (cursor) { const next = cursor.nextSibling; current.removeChild(cursor); cursor = next; }
}
export function patchMarkup(target: Element, markup: string) {
  const range = document.createRange(); range.selectNodeContents(target);
  patchChildren(target, range.createContextualFragment(markup));
}
export interface SurfaceRequest {
  owner: unknown;
  parent?: SurfaceHandle;
  expiresAt?: number;
  isValid?: () => boolean;
  show: () => void;
  hide: () => void;
  restoreFocus: () => void;
  onClose?: () => void;
}
export interface SurfaceHandle { close(): void; readonly visible: boolean; readonly closed: boolean }
/** FIFO decisions, with explicit child surfaces temporarily replacing their parent. No timers are needed to test expiry. */
export class SurfaceQueue {
  private waiting: Entry[] = [];
  private stack: Entry[] = [];
  private paused = 0;
  constructor(private now = () => Date.now()) {}
  private valid(entry: Entry) { return !entry.closed && (entry.request.expiresAt === undefined || entry.request.expiresAt > this.now()) && (entry.request.isValid?.() ?? true); }
  enqueue(request: SurfaceRequest): SurfaceHandle {
    const entry: Entry = { request, closed: false, visible: false, close: () => this.close(entry) };
    const parent = this.stack.at(-1);
    if (request.parent && request.parent === parent) {
      parent.visible = false; parent.request.hide(); this.stack.push(entry);
    } else this.waiting.push(entry);
    this.refresh(); return entry;
  }
  /** Polling owners call refresh when a generation or deadline changes. */
  refresh() {
    for (const entry of [...this.waiting, ...this.stack]) if (!this.valid(entry)) this.close(entry);
    if (this.paused) return;
    if (!this.stack.length) { const next = this.waiting.shift(); if (next) this.stack.push(next); }
    const top = this.stack.at(-1);
    if (top && !top.visible) { top.visible = true; top.request.show(); }
  }
  pause() { this.paused++; return () => { this.paused--; this.refresh(); }; }
  cancelOwner(owner: unknown) { const resume = this.pause(); for (const entry of [...this.waiting, ...this.stack].reverse()) if (entry.request.owner === owner) this.close(entry); resume(); }
  cancelAll() { const resume = this.pause(); for (const entry of [...this.waiting, ...this.stack].reverse()) this.close(entry); resume(); }
  private close(entry: Entry) {
    if (entry.closed) return;
    const wasVisible = entry.visible;
    entry.closed = true; entry.visible = false;
    this.waiting = this.waiting.filter(item => item !== entry); this.stack = this.stack.filter(item => item !== entry);
    if (wasVisible) entry.request.hide();
    entry.request.onClose?.();
    if (wasVisible) entry.request.restoreFocus();
    this.refresh();
  }
}
interface Entry extends SurfaceHandle { request: SurfaceRequest; closed: boolean; visible: boolean }

const surfaces = new SurfaceQueue();
const handles = new Map<HTMLDialogElement, SurfaceHandle>();
const restoreTargets = new WeakMap<HTMLDialogElement, HTMLElement | null>();
let surfaceOwner: unknown = 'page', sheetLabel = () => 'Close', sequence = 0;
let historyResume: (() => void) | undefined;
/** Dispose a page's surfaces before switching route. Polling prompts use their own owner. */
export function setSurfaceOwner(owner: unknown) { const previous = surfaceOwner; surfaceOwner = owner; surfaces.cancelOwner(previous); }
export function cancelSurfaces(owner?: unknown) { if (owner === undefined) surfaces.cancelAll(); else surfaces.cancelOwner(owner); }
export function refreshSurfaces() { surfaces.refresh(); }
export function closeSurface(dialog: HTMLDialogElement) { const handle = handles.get(dialog); if (handle) handle.close(); else { dialog.close(); dialog.remove(); } }
export interface SurfaceOptions { owner?: unknown; expiresAt?: number; isValid?: () => boolean; parent?: HTMLDialogElement | null }
/**
 * Open any decision or info sheet through this coordinator. FIFO by default. Pass parent: null for
 * unsolicited polling prompts (e.g. radio/Carlink), plus owner and isValid/expiry to drop stale work.
 * User-triggered children inherit the focused sheet as parent, suspending it until the child closes.
 * closeSurface ends a request; cancelSurfaces(owner) disposes a flow. Back dismisses only the visible sheet.
 */
export function openSurface(dialog: HTMLDialogElement, options: SurfaceOptions = {}): SurfaceHandle {
  const existing = handles.get(dialog); if (existing) return existing;
  let trigger = document.activeElement as HTMLElement | null;
  const focusedSheet = trigger?.closest('dialog');
  const parent = options.parent === undefined ? focusedSheet : options.parent;
  if (options.parent === null && focusedSheet) trigger = restoreTargets.get(focusedSheet) ?? null;
  const triggerId = trigger?.id; restoreTargets.set(dialog, trigger);
  const token = String(++sequence);
  let pushed = false;
  dialog.dataset.historySheet = token; dialog.classList.add('sheet'); dialog.tabIndex = -1; dialog.setAttribute('autofocus', '');
  const dismiss = () => { const event = new Event('cancel', { cancelable: true }); if (dialog.dispatchEvent(event)) closeSurface(dialog); };
  const handleButton = document.createElement('button'); handleButton.type = 'button'; handleButton.className = 'sheet-handle'; handleButton.setAttribute('aria-label', sheetLabel());
  handleButton.addEventListener('click', dismiss); dialog.prepend(handleButton);
  let down = 0;
  handleButton.addEventListener('pointerdown', event => { down = event.clientY; handleButton.setPointerCapture(event.pointerId); });
  handleButton.addEventListener('pointerup', event => { if (event.clientY - down > 40) dismiss(); });
  dialog.addEventListener('click', event => { if (event.target === dialog) { const r = dialog.getBoundingClientRect(); if (event.clientX < r.left || event.clientX > r.right || event.clientY < r.top || event.clientY > r.bottom) dismiss(); } });
  dialog.addEventListener('keydown', event => {
    if (event.key !== 'Tab') return;
    const nodes = [...dialog.querySelectorAll<HTMLElement>('button:not(:disabled), a[href], input:not(:disabled), select, textarea, [tabindex="0"]')].filter(node => node.getClientRects().length);
    const first = nodes[0], last = nodes.at(-1);
    if (event.shiftKey && (document.activeElement === first || document.activeElement === dialog)) { event.preventDefault(); last?.focus(); }
    else if (!event.shiftKey && (document.activeElement === last || document.activeElement === dialog)) { event.preventDefault(); first?.focus(); }
  });
  if (!dialog.isConnected) document.body.append(dialog);
  const handle = surfaces.enqueue({ owner: options.owner ?? surfaceOwner, parent: parent ? handles.get(parent) : undefined,
    expiresAt: options.expiresAt, isValid: options.isValid,
    show: () => {
      if (!dialog.isConnected) document.body.append(dialog);
      if (!pushed) { history.pushState({ ...history.state, oxpSheet: token }, '', location.href); pushed = true; }
      dialog.showModal(); dialog.focus();
    },
    hide: () => {
      if (dialog.open) dialog.close();
      // A suspended parent keeps its entry; only finished decisions consume history.
      if (handles.get(dialog)?.closed && history.state?.oxpSheet === token && !historyResume) { historyResume = surfaces.pause(); history.back(); }
    },
    restoreFocus: () => (trigger?.isConnected ? trigger : triggerId ? document.getElementById(triggerId) : null)?.focus(),
    onClose: () => { handles.delete(dialog); dialog.remove(); },
  });
  if (!handle.closed) handles.set(dialog, handle);
  dialog.addEventListener('close', () => { if (!dialog.open && handle.visible) handle.close(); });
  return handle;
}
export function enhanceSheets(label: () => string) {
  sheetLabel = label;
  window.addEventListener('popstate', () => {
    if (historyResume && history.state?.oxpSheet && ![...handles.keys()].some(dialog => dialog.dataset.historySheet === history.state.oxpSheet)) { history.back(); return; }
    if (historyResume) { const resume = historyResume; historyResume = undefined; resume(); return; }
    const current = [...handles].find(([, handle]) => handle.visible);
    if (current && history.state?.oxpSheet !== current[0].dataset.historySheet) {
      const event = new Event('cancel', { cancelable: true }); if (current[0].dispatchEvent(event)) current[1].close();
    }
  });
  new MutationObserver(() => { for (const [dialog, handle] of handles) if (!dialog.isConnected) handle.close(); }).observe(document.body, { childList: true, subtree: true });
}
/** A destructive decision gives Cancel the primary treatment. */
export function confirmSheet(message: string, primary: string, cancel: string, tone: 'default' | 'destructive' = 'default'): Promise<boolean> {
  const dialog = document.createElement('dialog'); dialog.setAttribute('aria-label', primary);
  patchMarkup(dialog, `<h2>${esc(primary)}</h2><p>${esc(message)}</p><div class="dialog-actions"><button class="${tone === 'destructive' ? 'primary' : 'quiet'}" data-close>${esc(cancel)}</button><button class="${tone === 'destructive' ? 'quiet destructive' : 'primary'}" data-confirm>${esc(primary)}</button></div>`);
  return new Promise(resolve => {
    let accepted = false;
    on(dialog.querySelector('[data-confirm]'), 'click', () => { accepted = true; closeSurface(dialog); });
    on(dialog.querySelector('[data-close]'), 'click', () => closeSurface(dialog));
    on(dialog, 'cancel', event => { event.preventDefault(); closeSurface(dialog); });
    const handle = openSurface(dialog);
    const observer = new MutationObserver(() => { if (handle.closed) { observer.disconnect(); if (history.state?.oxpSheet === dialog.dataset.historySheet) window.addEventListener('popstate', () => resolve(accepted), { once: true }); else resolve(accepted); } });
    observer.observe(document.body, { childList: true, subtree: true });
  });
}
