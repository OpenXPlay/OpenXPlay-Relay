// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-30

import { readFileSync } from 'node:fs';
import { expect, it } from 'vitest';
import { translate, type Language } from '../src/i18n';
import { esc } from '../src/html';
import { presentPath } from '../src/path/present-path';
import { presentStage } from '../src/path/present-stage';
import { stepDetail, stepRow, tickDeadlines } from '../src/stage';
import { stepDetailsView } from '../src/step-details';

const fixture = JSON.parse(readFileSync(new URL('./fixtures/linux/status-car-first-wait.json', import.meta.url), 'utf8'));
const input = fixture.pathInput, now: number = input.nowMillis;
const steps = presentPath(input).sections.flatMap(section => section.steps);
const waiting = steps.find(step => step.carWaitUntilMillis != null)!;
const translator = (language: Language) => (key: string, args?: Record<string, string | number>) => translate(language, key, args);

it('counts the car-first wait in drawer rows and the step sheet, like the headline', () => {
  expect(waiting.id).toBe('CARPLAY_START');
  const seconds = Math.ceil((waiting.carWaitUntilMillis! - now) / 1000);
  for (const language of ['en', 'zh-CN'] as const) {
    const t = translator(language), wait = (arg1: number) => t('path_car_screen_wait', { arg1 });
    expect(stepDetail(waiting, t, now)).toBe(wait(seconds));
    expect(stepDetail(waiting, t, now + 5500)).toBe(wait(seconds - 5));
    expect(stepDetail(waiting, t, waiting.carWaitUntilMillis! + 60000)).toBe(wait(0));
    for (const markup of [stepRow(waiting, 'linux', t, now + 1000), stepDetailsView(waiting, [], 'linux', t, now + 1000)]) {
      expect(markup).toContain(`data-deadline="CARPLAY_START">${esc(wait(seconds - 1))}<`);
      expect(markup).not.toContain('{arg1}');
    }
    // Steps without a deadline keep their plain text and are never rewritten by the tick.
    const other = steps.find(step => step.carWaitUntilMillis == null && step.detail)!;
    expect(stepRow(other, 'linux', t, now)).not.toContain('data-deadline');
    expect(stepDetail(other, t, now)).toBe(t(other.detail!.res!, Object.fromEntries((other.detail!.args ?? []).map((v, i) => [`arg${i + 1}`, v]))));
  }
  const view = presentStage(presentPath(input));
  expect(view.headlineDeadlineMillis).toBe(waiting.carWaitUntilMillis);
});

it('ticks counted details in place without rebuilding drawer rows or the step sheet', () => {
  const t = translator('en'), seconds = Math.ceil((waiting.carWaitUntilMillis! - now) / 1000);
  const element = (deadline: string, initial: string) => {
    let text = initial, writes = 0;
    return { dataset: { deadline }, get textContent() { return text; }, set textContent(value: string) { writes++; text = value; }, get writes() { return writes; } };
  };
  const counted = element('CARPLAY_START', 'stale'), unknown = element('TESLA', 'kept'), plain = element('MODE', 'kept');
  const selectors: string[] = [];
  const root = { querySelectorAll: (selector: string) => { selectors.push(selector); return [counted, unknown, plain]; } };
  tickDeadlines(root as unknown as ParentNode, steps, t, now + 3000);
  expect(counted.textContent).toBe(t('path_car_screen_wait', { arg1: seconds - 3 }));
  tickDeadlines(root as unknown as ParentNode, steps, t, now + 3400);
  expect(counted.writes).toBe(1);
  tickDeadlines(root as unknown as ParentNode, steps, t, now + 4000);
  expect(counted.textContent).toBe(t('path_car_screen_wait', { arg1: seconds - 4 }));
  expect(counted.writes).toBe(2);
  expect([unknown.textContent, plain.textContent, unknown.writes, plain.writes]).toEqual(['kept', 'kept', 0, 0]);
  expect(selectors).toEqual(['[data-deadline]', '[data-deadline]', '[data-deadline]']);
  expect(() => tickDeadlines(null, steps, t, now)).not.toThrow();
});
