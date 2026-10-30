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
  /** A higher priority preempts the visible surface, which resumes when it closes; FIFO within one priority. */
  priority?: number;
  /** A child suspends its parent until it closes; closing the parent closes the child. */
  parent?: SurfaceHandle;
  expiresAt?: number;
  isValid?: () => boolean;
  show: () => void;
  /** closing is false while the surface is only suspended under a child or a preempting prompt. */
  hide: (closing: boolean) => void;
  restoreFocus: () => void;
  onClose?: () => void;
}
export interface SurfaceHandle { close(): void; readonly visible: boolean; readonly closed: boolean }
interface Entry extends SurfaceHandle { request: SurfaceRequest; priority: number; parent?: Entry; closed: boolean; visible: boolean }
/** One visible surface at a time. Expiry is checked on refresh, so tests need no timers. */
export class SurfaceQueue {
  private waiting: Entry[] = [];
  private stack: Entry[] = [];
  private paused = 0;
  private focus: Entry | undefined;
  constructor(private now = () => Date.now()) {}
  private valid(entry: Entry) { return !entry.closed && (entry.request.expiresAt === undefined || entry.request.expiresAt > this.now()) && (entry.request.isValid?.() ?? true); }
  enqueue(request: SurfaceRequest): SurfaceHandle {
    const parent = this.stack.find(entry => entry === request.parent);
    const entry: Entry = { request, priority: Math.max(request.priority ?? 0, parent?.priority ?? 0), parent, closed: false, visible: false, close: () => this.close(entry) };
    if (parent) this.stack.splice(this.stack.indexOf(parent) + 1, 0, entry);
    else { const at = this.waiting.findIndex(other => other.priority < entry.priority); this.waiting.splice(at < 0 ? this.waiting.length : at, 0, entry); }
    this.refresh(); return entry;
  }
  /** Polling owners call refresh when a generation or deadline changes. */
  refresh() {
    for (const entry of [...this.waiting, ...this.stack]) if (!this.valid(entry)) this.drop(entry);
    if (this.paused) return;
    const next = this.waiting[0], top = this.stack.at(-1);
    if (next && (!top || next.priority > top.priority)) { this.waiting.shift(); this.stack.push(next); }
    const current = this.stack.at(-1);
    for (const entry of this.stack) if (entry !== current && entry.visible) { entry.visible = false; entry.request.hide(false); }
    if (current && !current.visible) { current.visible = true; current.request.show(); }
    // Focus returns to the trigger only when nothing else claimed it, or when a child hands back to its parent.
    const closed = this.focus; this.focus = undefined;
    if (closed && (!current || current === closed.parent)) closed.request.restoreFocus();
  }
  /** Holds the next surface back, e.g. while the browser finishes a history step. */
  pause() { this.paused++; let resumed = false; return () => { if (resumed) return; resumed = true; this.paused--; this.refresh(); }; }
  cancelOwner(owner: unknown) { const resume = this.pause(); for (const entry of [...this.waiting, ...this.stack].reverse()) if (entry.request.owner === owner) this.drop(entry); resume(); }
  cancelAll() { const resume = this.pause(); for (const entry of [...this.waiting, ...this.stack].reverse()) this.drop(entry); resume(); }
  private close(entry: Entry) { this.drop(entry); this.refresh(); }
  private drop(entry: Entry) {
    if (entry.closed) return;
    const before = this.focus;
    for (const child of this.stack.filter(other => other.parent === entry)) this.drop(child);
    // A visible child closing with its parent hands focus back to the parent's own trigger.
    const shown = entry.visible || this.focus !== before;
    entry.closed = true;
    this.waiting = this.waiting.filter(item => item !== entry); this.stack = this.stack.filter(item => item !== entry);
    if (entry.visible) { entry.visible = false; entry.request.hide(true); }
    if (shown) this.focus = entry;
    entry.request.onClose?.();
  }
}
const surfaces = new SurfaceQueue();
const handles = new Map<HTMLDialogElement, SurfaceHandle>();
const restoreTargets = new WeakMap<HTMLDialogElement, HTMLElement | null>();
const decorated = new WeakSet<HTMLDialogElement>();
// Captured before enhanceSheets routes showModal() through the queue.
const nativeShowModal = typeof HTMLDialogElement === 'undefined' ? undefined : HTMLDialogElement.prototype.showModal;
let surfaceOwner: unknown = 'page', sheetLabel = () => 'Close', sequence = 0;
let historyResume: (() => void) | undefined;
/** Dispose a page's surfaces before switching route. Prompts use their own owner. */
export function setSurfaceOwner(owner: unknown) { const previous = surfaceOwner; surfaceOwner = owner; surfaces.cancelOwner(previous); }
export function cancelSurfaces(owner?: unknown) { if (owner === undefined) surfaces.cancelAll(); else surfaces.cancelOwner(owner); }
export function refreshSurfaces() { surfaces.refresh(); }
export function closeSurface(dialog: HTMLDialogElement) { const handle = handles.get(dialog); if (handle) handle.close(); else { dialog.close(); dialog.remove(); } }
export interface SurfaceOptions { owner?: unknown; priority?: number; parent?: HTMLDialogElement | null; expiresAt?: number; isValid?: () => boolean; onClose?: () => void }
export interface PromptOptions { owner: unknown; priority?: number; expiresAt?: number; isValid?: () => boolean; onClose?: () => void }
/** Priorities: ordinary sheets and suggestions queue FIFO; a time-limited prompt preempts them. */
export const SURFACE_PRIORITY = { sheet: 0, urgent: 1 } as const;
/**
 * Opens a decision or sheet through the one queue; a plain dialog.showModal() lands here too.
 * A sheet opened from inside the visible sheet becomes its child and suspends it until it closes.
 * closeSurface ends one surface, cancelSurfaces(owner) a whole flow, and browser Back only the visible one.
 */
export function openSurface(dialog: HTMLDialogElement, options: SurfaceOptions = {}): SurfaceHandle {
  const existing = handles.get(dialog); if (existing) return existing;
  const active = document.activeElement instanceof HTMLElement ? document.activeElement : null;
  const focusedSheet = active?.closest('dialog') ?? null;
  const parent = options.parent === undefined ? focusedSheet : options.parent;
  // A prompt nobody asked for returns focus to what was under the sheet it waited behind.
  const trigger = options.parent === null && focusedSheet ? restoreTargets.get(focusedSheet) ?? null : active;
  const triggerId = trigger?.id; restoreTargets.set(dialog, trigger);
  const token = String(++sequence); dialog.dataset.historySheet = token;
  let pushed = false, lastFocus: HTMLElement | null = null;
  decorate(dialog);
  if (!dialog.isConnected) document.body.append(dialog);
  const handle = surfaces.enqueue({ owner: options.owner ?? surfaceOwner, priority: options.priority, parent: parent ? handles.get(parent) : undefined,
    expiresAt: options.expiresAt, isValid: options.isValid,
    show: () => {
      if (!dialog.isConnected) document.body.append(dialog);
      if (!pushed) { history.pushState({ ...history.state, oxpSheet: token }, '', location.href); pushed = true; }
      if (!dialog.open) nativeShowModal?.call(dialog);
      // The sheet itself takes focus so a touch keyboard stays down; a resumed sheet returns to its last control.
      (lastFocus?.isConnected && dialog.contains(lastFocus) ? lastFocus : dialog).focus();
    },
    hide: closing => {
      if (dialog.contains(document.activeElement)) lastFocus = document.activeElement as HTMLElement;
      if (dialog.open) dialog.close();
      // A suspended sheet keeps its history entry; a finished one gives it back.
      if (closing) leaveHistory(token);
    },
    restoreFocus: () => (trigger?.isConnected ? trigger : triggerId ? document.getElementById(triggerId) : null)?.focus(),
    onClose: () => { handles.delete(dialog); dialog.remove(); options.onClose?.(); },
  });
  if (!handle.closed) handles.set(dialog, handle);
  return handle;
}
/** For decisions that arrive on their own (Carlink PIN, radio suggestions, set a password): never a child of the visible sheet. */
export function openPrompt(dialog: HTMLDialogElement, options: PromptOptions): SurfaceHandle { return openSurface(dialog, { ...options, parent: null }); }
function decorate(dialog: HTMLDialogElement) {
  if (decorated.has(dialog)) return; decorated.add(dialog);
  dialog.classList.add('sheet'); dialog.tabIndex = -1; dialog.setAttribute('autofocus', '');
  // The primary action comes first in reading and focus order, as it does on screen.
  dialog.querySelectorAll<HTMLElement>('.dialog-actions').forEach(actions => { const primary = actions.querySelector(':scope > .primary, :scope > .confirm, :scope > [type="submit"]'); if (primary) actions.prepend(primary); });
  const dismiss = () => { if (dialog.dispatchEvent(new Event('cancel', { cancelable: true }))) closeSurface(dialog); };
  const grip = document.createElement('button'); grip.type = 'button'; grip.className = 'sheet-handle'; grip.setAttribute('aria-label', sheetLabel());
  grip.addEventListener('click', dismiss); dialog.prepend(grip);
  let down = 0;
  grip.addEventListener('pointerdown', event => { down = event.clientY; grip.setPointerCapture(event.pointerId); });
  grip.addEventListener('pointerup', event => { if (event.clientY - down > 40) dismiss(); });
  dialog.addEventListener('click', event => { if (event.target === dialog) { const r = dialog.getBoundingClientRect(); if (event.clientX < r.left || event.clientX > r.right || event.clientY < r.top || event.clientY > r.bottom) dismiss(); } });
  dialog.addEventListener('keydown', event => {
    if (event.key !== 'Tab') return;
    const nodes = [...dialog.querySelectorAll<HTMLElement>('button:not(:disabled), a[href], input:not(:disabled), select, textarea, [tabindex="0"]')].filter(node => node.getClientRects().length);
    const first = nodes[0], last = nodes.at(-1);
    if (event.shiftKey && (document.activeElement === first || document.activeElement === dialog)) { event.preventDefault(); last?.focus(); }
    else if (!event.shiftKey && (document.activeElement === last || document.activeElement === dialog)) { event.preventDefault(); first?.focus(); }
  });
  // Escape and direct dialog.close() calls end the visible surface; a suspended one closes silently.
  dialog.addEventListener('close', () => { const handle = handles.get(dialog); if (!dialog.open && handle?.visible) handle.close(); });
}
function leaveHistory(token: string) {
  if (history.state?.oxpSheet !== token || historyResume) return;
  const resume = surfaces.pause();
  const timer = setTimeout(() => historyResume?.(), 1000);
  historyResume = () => { clearTimeout(timer); historyResume = undefined; resume(); };
  history.back();
}
export function enhanceSheets(label: () => string) {
  sheetLabel = label;
  if (!nativeShowModal || HTMLDialogElement.prototype.showModal !== nativeShowModal) return;
  // Modules that open their own dialogs join the queue without importing it.
  HTMLDialogElement.prototype.showModal = function (this: HTMLDialogElement) { openSurface(this); };
  window.addEventListener('popstate', () => {
    const token = history.state?.oxpSheet;
    if (historyResume) {
      // Skip entries left by sheets that closed while suspended.
      if (token && ![...handles.keys()].some(dialog => dialog.dataset.historySheet === token)) history.back(); else historyResume();
      return;
    }
    const current = [...handles].find(([, handle]) => handle.visible);
    if (current && token !== current[0].dataset.historySheet && current[0].dispatchEvent(new Event('cancel', { cancelable: true }))) current[1].close();
  });
  new MutationObserver(() => { for (const [dialog, handle] of handles) if (!dialog.isConnected) handle.close(); }).observe(document.body, { childList: true, subtree: true });
}
/** A destructive decision gives Cancel the primary treatment and the action a quiet red label. */
export function confirmSheet(message: string, primary: string, cancel: string, tone: 'default' | 'destructive' = 'default'): Promise<boolean> {
  const dialog = document.createElement('dialog'); dialog.setAttribute('aria-label', primary);
  const destructive = tone === 'destructive';
  patchMarkup(dialog, `<h2>${esc(primary)}</h2><p>${esc(message)}</p><div class="dialog-actions"><button class="${destructive ? 'quiet destructive' : 'primary'}" data-confirm>${esc(primary)}</button><button class="${destructive ? 'primary' : 'quiet'}" data-close>${esc(cancel)}</button></div>`);
  return new Promise(resolve => {
    let accepted = false;
    on(dialog.querySelector('[data-confirm]'), 'click', () => { accepted = true; closeSurface(dialog); });
    on(dialog.querySelector('[data-close]'), 'click', () => closeSurface(dialog));
    on(dialog, 'cancel', event => { event.preventDefault(); closeSurface(dialog); });
    openSurface(dialog, { onClose: () => resolve(accepted) });
  });
}
