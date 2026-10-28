// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-27

import { expect, it, vi } from 'vitest';
vi.mock('../src/dom', () => ({ patchMarkup: (el: { innerHTML: string }, html: string) => { el.innerHTML = html; }, on: (el: { handlers: Record<string, (event?: unknown) => void> }, event: string, handler: () => void) => { if (el) el.handlers[event] = handler; } }));
import { mountPictureField, pictureRatio } from '../src/picture-sizes';
import type { Values } from '../src/schema';
function field(values: Values, current?: string) {
  const choice = { dataset: { size: '2' }, handlers: {} as Record<string, () => void> };
  const remove = { dataset: { delete: '0' }, handlers: {} as Record<string, () => void> };
  const form = { handlers: {}, elements: {} };
  const target = { innerHTML: '', querySelectorAll: (selector: string) => selector === '[data-size]' ? [choice] : [remove], querySelector: (selector: string) => selector === 'form' ? form : null };
  const save = vi.fn().mockResolvedValue(undefined);
  mountPictureField(target as unknown as HTMLElement, { key: 'carlinkPictureSizes', type: 'picture-sizes', labelKey: '' }, values, key => key, save, current);
  return { target, save, choice, remove };
}
it('selects and deletes the active trial immediately while current waits for playback', async () => {
  const size = { width: 1300, height: 720 };
  const values = { output: 'both', carlinkPictureSizes: [size], carlinkPictureTry: size };
  const f = field(values);
  f.choice.handlers.click(); await Promise.resolve();
  expect(f.save).toHaveBeenCalledWith({ carlinkPictureTry: { width: 1920, height: 1080 } });
  expect(f.target.innerHTML).toContain('picture.pending'); expect(f.target.innerHTML).not.toContain('picture.current: 1920');
  values.carlinkPictureTry = size; f.remove.handlers.click(); await Promise.resolve();
  expect(f.save).toHaveBeenLastCalledWith({ carlinkPictureSizes: [], carlinkPictureTry: null });
  expect(field(values, '1280 × 720').target.innerHTML).toContain('picture.current: 1280 × 720');
  expect(pictureRatio({ width: 1920, height: 1080 })).toBe('16:9');
});
it('shows the Android availability hints and disables unavailable size choices', () => {
  expect(field({ output: 'web' }).target.innerHTML).toContain('picture.outputOff');
  expect(field({ output: 'both', displayMode: 'fixed' }).target.innerHTML).toContain('picture.fixed');
  expect(field({ output: 'web' }).target.innerHTML).toContain('<fieldset disabled>');
});
