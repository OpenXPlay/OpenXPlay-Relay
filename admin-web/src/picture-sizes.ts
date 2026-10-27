// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-27

import type { PictureSize, Field, Value, Values } from './schema';
import { esc } from './html';
import { on, patchMarkup } from './dom';
import type { Translate } from './stage';
export const picturePresets: PictureSize[] = [[1280, 720], [1920, 1080], [1280, 800], [1920, 720], [1024, 600], [1024, 768]].map(([width, height]) => ({ width, height }));
export function validPictureSize(size: PictureSize): boolean {
  const { width: w, height: h } = size, long = Math.max(w, h), short = Math.min(w, h);
  return Number.isInteger(w) && Number.isInteger(h) && w % 2 === 0 && h % 2 === 0 && long >= 640 && long <= 1920 && short >= 360 && short <= 1080;
}
const same = (a: PictureSize | null, b: PictureSize | null) => a?.width === b?.width && a?.height === b?.height;
export function normalizedSizes(sizes: PictureSize[]): PictureSize[] { return sizes.filter((s, i) => validPictureSize(s) && !picturePresets.some(p => same(p, s)) && !sizes.slice(0, i).some(p => same(p, s))).slice(0, 5); }
export function mountPictureField(target: HTMLElement, field: Field, values: Values, t: Translate, save: (field: Field, value: Value) => void) {
  const sizes = () => Array.isArray(values.carlinkPictureSizes) ? values.carlinkPictureSizes as PictureSize[] : [];
  const label = (size: PictureSize | null) => size ? `${size.width} × ${size.height}` : t('ui.automatic');
  const choose = (size: PictureSize | null) => { save(field, size); if (field.key !== 'carlinkPictureTry') values[field.key] = size; render(); };
  function render() {
    if (field.key === 'carlinkPictureSizes') {
      patchMarkup(target, `<h3>${esc(t(field.labelKey))}</h3><p class="muted">${esc(t('picture.customHint'))}</p><div>${sizes().map((s, index) => `<div class="address-row" data-key="size-${index}"><span>${label(s)}</span><button data-delete="${index}" aria-label="${esc(t('net.delete'))}: ${label(s)}">${t('net.delete')}</button></div>`).join('')}</div><form class="picture-size-form"><label>${t('picture.width')}<input name="width" type="number" step="2" min="360" max="1920" inputmode="numeric" required></label><label>${t('picture.height')}<input name="height" type="number" step="2" min="360" max="1920" inputmode="numeric" required></label><button ${sizes().length >= 5 ? 'disabled' : ''}>${t('net.save')}</button><p class="error" role="alert" data-picture-error></p></form>`);
      target.querySelectorAll<HTMLButtonElement>('[data-delete]').forEach(button => on(button, 'click', () => { const removed = sizes()[Number(button.dataset.delete)], next = sizes().filter(s => !same(s, removed)); values.carlinkPictureSizes = next; save(field, next); if (same(values.carlinkPictureDefault as PictureSize | null, removed)) { values.carlinkPictureDefault = null; const defaultField = { ...field, key: 'carlinkPictureDefault' }; save(defaultField, null); } render(); }));
      on(target.querySelector('form'), 'submit', event => {
        event.preventDefault(); const form = event.target as HTMLFormElement;
        const size = { width: Number((form.elements.namedItem('width') as HTMLInputElement).value), height: Number((form.elements.namedItem('height') as HTMLInputElement).value) };
        if (!validPictureSize(size)) { target.querySelector('[data-picture-error]')!.textContent = t('picture.invalid'); return; }
        const next = normalizedSizes([...sizes(), size]); values.carlinkPictureSizes = next; save(field, next); form.reset(); render();
      });
    } else {
      const current = values[field.key] as PictureSize | null;
      patchMarkup(target, `<h3>${esc(t(field.labelKey))}</h3><div class="chips">${[null, ...picturePresets, ...sizes()].map((s, index) => `<button data-size="${index}" aria-pressed="${same(s, current)}">${label(s)}</button>`).join('')}</div>${field.key === 'carlinkPictureTry' ? `<p class="muted">${t('picture.trialHint')}</p>` : ''}`);
      target.querySelectorAll<HTMLButtonElement>('[data-size]').forEach(button => on(button, 'click', () => { if (confirm(t('files.pictureRestart'))) choose([null, ...picturePresets, ...sizes()][Number(button.dataset.size)]); }));
    }
  }
  render();
}
