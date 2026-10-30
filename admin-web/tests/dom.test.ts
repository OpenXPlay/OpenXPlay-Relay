// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-30

import { expect, it } from 'vitest';
import { SurfaceQueue, type SurfaceHandle } from '../src/dom';
function fixture() {
  let now = 100, focused = 'page';
  const visible = new Set<string>(), shown: string[] = [], closed: string[] = [];
  const queue = new SurfaceQueue(() => now);
  const add = (name: string, owner = 'page', options: { parent?: SurfaceHandle; expiresAt?: number; isValid?: () => boolean } = {}) => {
    const trigger = focused;
    return queue.enqueue({ owner, ...options, show: () => { visible.add(name); shown.push(name); focused = name; expect(visible.size).toBe(1); }, hide: () => { visible.delete(name); }, restoreFocus: () => { focused = trigger; }, onClose: () => closed.push(name) });
  };
  return { queue, add, visible, shown, closed, focus: () => focused, time: (value: number) => { now = value; } };
}
it('shows one surface and advances FIFO without restoring focus for unseen work', () => {
  const f = fixture(), a = f.add('a'), b = f.add('b'), c = f.add('c');
  expect(f.shown).toEqual(['a']); expect(b.visible).toBe(false);
  a.close(); expect(f.shown).toEqual(['a', 'b']); b.close(); c.close();
  expect(f.shown).toEqual(['a', 'b', 'c']); expect(f.visible.size).toBe(0);
});
it('drops expired and invalid queued prompts before showing them', () => {
  const f = fixture(), a = f.add('a'); let valid = true;
  const expired = f.add('expired', 'poll', { expiresAt: 200 });
  const stale = f.add('stale', 'poll', { isValid: () => valid }); f.add('last');
  f.time(200); valid = false; a.close();
  expect(expired.closed).toBe(true); expect(stale.closed).toBe(true); expect(f.shown).toEqual(['a', 'last']);
});
it('cancels a route owner without flashing its queued surfaces', () => {
  const f = fixture(); f.add('a', 'route'); f.add('b', 'route'); f.add('prompt', 'poll');
  f.queue.cancelOwner('route'); expect(f.shown).toEqual(['a', 'prompt']); expect(f.closed).toEqual(['a', 'b']);
  f.queue.cancelAll(); expect(f.visible.size).toBe(0);
});
it('suspends a parent for a child and restores the parent and original focus', () => {
  const f = fixture(), parent = f.add('parent'); f.add('poll', 'poll');
  const child = f.add('child', 'page', { parent }); expect(parent.visible).toBe(false);
  child.close(); expect(parent.visible).toBe(true); expect(f.focus()).toBe('parent');
  parent.close(); expect(f.shown).toEqual(['parent', 'child', 'parent', 'poll']);
});
it('restores the trigger on close and expires a visible prompt on refresh', () => {
  const f = fixture(); const a = f.add('a', 'poll', { expiresAt: 200 });
  f.time(200); f.queue.refresh(); expect(a.closed).toBe(true); expect(f.focus()).toBe('page');
});
it('holds the next surface until a browser history transition completes', () => {
  const f = fixture(), a = f.add('a'); f.add('b');
  const resume = f.queue.pause(); a.close(); expect(f.visible.size).toBe(0); resume(); expect(f.shown).toEqual(['a', 'b']);
});
