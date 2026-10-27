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
export function enhanceSheets(label: () => string) {
  let sequence = 0, dismissingHistory = false;
  const dismiss = (dialog: HTMLDialogElement) => { const event = new Event('cancel', { cancelable: true }); if (dialog.dispatchEvent(event)) { dialog.close(); dialog.remove(); } };
  window.addEventListener('popstate', () => {
    if (dismissingHistory) { dismissingHistory = false; return; }
    const dialog = [...document.querySelectorAll<HTMLDialogElement>('dialog[open]')].at(-1);
    if (dialog && history.state?.oxpSheet !== dialog.dataset.historySheet) dismiss(dialog);
  });
  const enhance = (dialog: HTMLDialogElement) => {
    if (dialog.dataset.sheetReady) return;
    dialog.querySelectorAll<HTMLElement>('.dialog-actions').forEach(actions => {
      const primary = actions.querySelector('.primary, .confirm, button[type="submit"]');
      if (primary) actions.prepend(primary);
    });
    dialog.dataset.sheetReady = '1'; dialog.classList.add('sheet');
    const historySheet = String(++sequence); dialog.dataset.historySheet = historySheet;
    history.pushState({ ...history.state, oxpSheet: historySheet }, '', location.href);
    dialog.addEventListener('close', () => { if (history.state?.oxpSheet === historySheet) { dismissingHistory = true; history.back(); } });
    const handle = document.createElement('button'); handle.type = 'button'; handle.className = 'sheet-handle'; handle.setAttribute('aria-label', label());
    const close = () => dismiss(dialog);
    handle.addEventListener('click', close); dialog.prepend(handle);
    let down = 0;
    handle.addEventListener('pointerdown', e => { down = e.clientY; handle.setPointerCapture(e.pointerId); });
    handle.addEventListener('pointerup', e => { if (e.clientY - down > 40) close(); });
    dialog.addEventListener('click', e => { if (e.target === dialog) { const r = dialog.getBoundingClientRect(); if (e.clientX < r.left || e.clientX > r.right || e.clientY < r.top || e.clientY > r.bottom) close(); } });
    dialog.addEventListener('keydown', e => {
      if (e.key !== 'Tab') return;
      const nodes = [...dialog.querySelectorAll<HTMLElement>('button:not(:disabled), a[href], input:not(:disabled), select, textarea, [tabindex="0"]')].filter(n => n.getClientRects().length);
      const first = nodes[0], last = nodes.at(-1);
      if (e.shiftKey && document.activeElement === first) { e.preventDefault(); last?.focus(); }
      if (!e.shiftKey && document.activeElement === last) { e.preventDefault(); first?.focus(); }
    });
  };
  new MutationObserver(() => document.querySelectorAll<HTMLDialogElement>('dialog').forEach(enhance)).observe(document.body, { childList: true, subtree: true });
}

/** A shared confirmation sheet keeps primary and quiet actions in reading and focus order. */
export function confirmSheet(message: string, primary: string, cancel: string): Promise<boolean> {
  const trigger = document.activeElement as HTMLElement | null;
  const dialog = document.createElement('dialog'); dialog.setAttribute('aria-label', primary);
  patchMarkup(dialog, `<h2>${esc(primary)}</h2><p>${esc(message)}</p><div class="dialog-actions"><button class="primary" data-confirm>${esc(primary)}</button><button class="quiet" data-close>${esc(cancel)}</button></div>`);
  return new Promise(resolve => {
    let closed = false;
    const close = (accepted: boolean) => {
      if (closed) return;
      closed = true;
      // Finish removing this history entry before a caller closes its parent choice sheet.
      const historySheet = dialog.dataset.historySheet;
      const waitForBack = historySheet && history.state?.oxpSheet === historySheet;
      if (waitForBack) window.addEventListener('popstate', () => resolve(accepted), { once: true });
      dialog.close(); dialog.remove(); trigger?.focus();
      if (!waitForBack) resolve(accepted);
    };
    on(dialog.querySelector('[data-confirm]'), 'click', () => close(true));
    on(dialog.querySelector('[data-close]'), 'click', () => close(false));
    on(dialog, 'cancel', event => { event.preventDefault(); close(false); });
    document.body.append(dialog); dialog.showModal();
  });
}
