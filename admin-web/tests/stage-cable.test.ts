// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-30

import { afterEach, expect, it, vi } from 'vitest';
import { cableMarkup, cableMotion, playCableMotion, type LinkFill } from '../src/stage-cable';
import { translate } from '../src/i18n';
const t = (key: string, args?: Record<string, string | number>) => translate('en', key, args);
const partial: LinkFill = { done: 2, total: 4, state: 'WORKING' };
afterEach(() => vi.unstubAllGlobals());
it('fills the sag from the relay outwards, labels counts and fades them when live', () => {
  const left = cableMarkup(partial, true, false, 'iPhone', t);
  expect(left).toContain('M0 12 C30 22 70 22 100 12');
  expect(left).toContain('translate(100 0) scale(-1 1)');
  expect(left).toContain('stroke-dasharray="0.5 1"');
  expect(left).toContain('2/4'); expect(left).toContain('iPhone, 2 of 4 steps');
  const right = cableMarkup(partial, false, true, 'Car', t);
  expect(right).not.toContain('scale(-1'); expect(right).toContain('data-live');
  expect(right).toContain('stroke-dasharray="1 1"');
  expect(right).toContain('data-complete="true"');
});
it('dashes blocked cables and leaves a gap in both the track and fill on failure', () => {
  expect(cableMarkup({ ...partial, state: 'BLOCKED' }, false, false, 'Car', t)).toContain('stroke-dasharray=".05 .04"');
  const failure = cableMarkup({ ...partial, state: 'FAILED' }, false, false, 'Car', t);
  expect(failure).toContain('stroke-dasharray="0.48 .04 0.48"');
  expect(failure).toContain('stroke-dasharray="0.48 0.52"');
  expect(cableMarkup({ done: 0, total: 0, state: 'BLOCKED' }, false, false, 'Car', t)).not.toContain('NaN');
});
it('schedules sparks only for newly finished steps, a completion click and one live entry', () => {
  const complete: LinkFill = { done: 4, total: 4, state: 'OK' };
  expect(cableMotion(null, partial, false, false)).toEqual({ sparks: 0, click: false, pulse: false });
  expect(cableMotion(partial, complete, false, true)).toEqual({ sparks: 2, click: true, pulse: true });
  expect(cableMotion(complete, complete, true, true)).toEqual({ sparks: 0, click: false, pulse: false });
  expect(cableMotion(complete, partial, true, false)).toEqual({ sparks: 0, click: false, pulse: false });
});
it('runs finite SVG passes in iPhone-to-relay-to-car order and snaps for reduced motion', () => {
  class Node {
    attributes: Record<string, string> = {}; children: Node[] = []; listeners: Record<string, () => void> = {};
    beginElementAt = vi.fn(); remove = vi.fn(); animate = vi.fn();
    setAttribute(name: string, value: string) { this.attributes[name] = value; }
    addEventListener(name: string, listener: () => void) { this.listeners[name] = listener; }
    append(child: Node) { this.children.push(child); }
    querySelector() { return this; }
  }
  const root = new Node(); vi.stubGlobal('document', { createElementNS: () => new Node() });
  const element = root as unknown as HTMLElement;
  playCableMotion(element, true, { sparks: 1, click: true, pulse: true }, true);
  expect(root.children).toHaveLength(0); expect(root.animate).not.toHaveBeenCalled();
  playCableMotion(element, true, { sparks: 1, click: true, pulse: true }, false);
  expect(root.children).toHaveLength(3); expect(root.animate).toHaveBeenCalledOnce();
  expect(root.children[0].children[0].attributes.path).toMatch(/^M100/);
  expect(root.children.slice(1).map(n => n.children[0].beginElementAt.mock.calls[0][0])).toEqual([0, 1.6]);
  for (const dot of root.children) {
    const motion = dot.children[0]; expect(motion.attributes.repeatCount).toBe('1');
    expect(motion.attributes.begin).toBe('indefinite'); motion.listeners.endEvent(); expect(dot.remove).toHaveBeenCalledOnce();
  }
  const right = new Node(); playCableMotion(right as unknown as HTMLElement, false, { sparks: 0, click: false, pulse: true }, false);
  expect(right.children.map(n => n.children[0].beginElementAt.mock.calls[0][0])).toEqual([.65, 2.25]);
});
