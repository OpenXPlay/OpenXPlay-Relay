// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-27

import type { PictureSize, Field, Values } from './schema';
import { esc } from './html';
import { on, patchMarkup } from './dom';
import type { Translate } from './stage';
export const picturePresets: PictureSize[] = [[1280, 720], [1920, 1080], [1280, 800], [1920, 720], [1024, 600], [1024, 768]].map(([width, height]) => ({ width, height }));
export function validPictureSize(size: PictureSize): boolean {
  const { width: w, height: h } = size, long = Math.max(w, h), short = Math.min(w, h);
  return Number.isInteger(w) && Number.isInteger(h) && w % 2 === 0 && h % 2 === 0 && long >= 640 && long <= 1920 && short >= 360 && short <= 1080;
}
export function pictureRatio(size: PictureSize): string {
  const gcd = (a: number, b: number): number => b ? gcd(b, a % b) : a;
  const divisor = gcd(size.width, size.height); return `${size.width / divisor}:${size.height / divisor}`;
}
const same = (a: PictureSize | null, b: PictureSize | null) => a?.width === b?.width && a?.height === b?.height;
export function normalizedSizes(sizes: PictureSize[]): PictureSize[] { return sizes.filter((s, i) => validPictureSize(s) && !picturePresets.some(p => same(p, s)) && !sizes.slice(0, i).some(p => same(p, s))).slice(0, 5); }
export function mountPictureField(target: HTMLElement, _field: Field, values: Values, t: Translate, save: (changes: Values) => Promise<void>, inUse?: string) {
  let selected = (Object.hasOwn(values, 'carlinkPictureTry') ? values.carlinkPictureTry : values.carlinkPictureDefault ?? null) as PictureSize | null;
  let width = '', height = '', busy = false, error = '';
  const currentPicture = inUse;
  const sizes = () => Array.isArray(values.carlinkPictureSizes) ? values.carlinkPictureSizes as PictureSize[] : [];
  const label = (size: PictureSize | null) => size ? `${size.width} × ${size.height}` : t('ui.automatic');
  async function apply(changes: Values) {
    busy = true; error = ''; render();
    try { await save(changes); Object.assign(values, changes); }
    catch { error = t('error.request'); }
    finally { busy = false; render(); }
  }
  function render() {
    const choices = [null, ...picturePresets, ...sizes()];
    const unavailable = values.output === 'web' ? 'picture.outputOff' : values.displayMode === 'fixed' ? 'picture.fixed' : '';
    patchMarkup(target, `<h2>${esc(t('picture.title'))}</h2><p class="muted" data-current-picture>${currentPicture ? `${esc(t('picture.current'))}: ${esc(currentPicture)}` : esc(t('picture.pending'))}</p>${unavailable ? `<p class="muted">${esc(t(unavailable))}</p>` : ''}<fieldset ${unavailable ? 'disabled' : ''}><div class="chips" role="group" aria-label="${esc(t('picture.title'))}">${choices.map((size, index) => `<button data-size="${index}" aria-pressed="${same(size, selected)}" ${busy ? 'disabled' : ''}>${label(size)}${size ? ` · ${pictureRatio(size)}` : ''}${same(size, values.carlinkPictureDefault as PictureSize | null) ? ` · ${esc(t('picture.default'))}` : ''}</button>`).join('')}</div><button class="quiet full-width" data-default ${busy ? 'disabled' : ''}>${esc(t('picture.setDefault'))} · ${label(selected)}</button><p class="muted">${esc(t('picture.trialHint'))}</p><div>${sizes().map((size, index) => `<div class="address-row" data-key="size-${index}"><span>${label(size)}</span><button class="quiet" data-delete="${index}" aria-label="${esc(t('net.delete'))}: ${label(size)}" ${busy ? 'disabled' : ''}>${esc(t('net.delete'))}</button></div>`).join('')}</div><p class="muted">${esc(t('picture.customHint'))}</p><form class="picture-size-form"><label>${esc(t('picture.width'))}<input name="width" type="number" step="2" min="360" max="1920" inputmode="numeric" value="${esc(width)}" required></label><label>${esc(t('picture.height'))}<input name="height" type="number" step="2" min="360" max="1920" inputmode="numeric" value="${esc(height)}" required></label><button class="pill" ${busy || sizes().length >= 5 ? 'disabled' : ''}>${esc(t('picture.add'))}</button><p class="error" role="alert" data-picture-error>${esc(error)}</p></form></fieldset>`);
    target.querySelectorAll<HTMLButtonElement>('[data-size]').forEach(button => on(button, 'click', () => { selected = choices[Number(button.dataset.size)]; void apply({ carlinkPictureTry: selected }); }));
    on(target.querySelector('[data-default]'), 'click', () => void apply({ carlinkPictureDefault: selected }));
    target.querySelectorAll<HTMLButtonElement>('[data-delete]').forEach(button => on(button, 'click', () => {
      const removed = sizes()[Number(button.dataset.delete)], next = sizes().filter(s => !same(s, removed));
      if (same(selected, removed)) selected = null;
      void apply({ carlinkPictureSizes: next, ...(same(values.carlinkPictureDefault as PictureSize | null, removed) ? { carlinkPictureDefault: null } : {}), ...(same(values.carlinkPictureTry as PictureSize | null, removed) ? { carlinkPictureTry: null } : {}) });
    }));
    const form = target.querySelector<HTMLFormElement>('form')!;
    on(form, 'input', () => { width = (form.elements.namedItem('width') as HTMLInputElement).value; height = (form.elements.namedItem('height') as HTMLInputElement).value; });
    on(form, 'submit', event => {
      event.preventDefault(); const size = { width: Number(width), height: Number(height) };
      if (!validPictureSize(size)) { error = t('picture.invalid'); render(); return; }
      if (choices.some(s => same(s, size))) { selected = size; width = ''; height = ''; void apply({ carlinkPictureTry: size }); return; }
      if (sizes().length >= 5) return;
      selected = size; width = ''; height = ''; void apply({ carlinkPictureSizes: [...sizes(), size], carlinkPictureTry: size });
    });
  }
  render();
}
