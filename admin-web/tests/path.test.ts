// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-27

import { expect, it } from 'vitest';
import { readFileSync } from 'node:fs';
import { presentPath } from '../src/path/present-path';
import { presentStage, stageTapSettled } from '../src/path/present-stage';
import type { ConnectionPath, PartialInput } from '../src/path/model';
const read = (name: string) => JSON.parse(readFileSync(new URL(`vectors/path/${name}.json`, import.meta.url), 'utf8'));
// Android's StepText has nullable resource/raw members; TS omits unused members.
function canonical(value: unknown): unknown {
  if (Array.isArray(value)) return value.map(canonical);
  if (value && typeof value === 'object') return Object.fromEntries(Object.entries(value).filter(([key, v]) => v !== null || !['res', 'raw'].includes(key)).filter(([key]) => key !== 'platform').map(([key, v]) => [key, key === 'args' && (v as unknown[])?.length === 0 ? undefined : canonical(v)]).filter(([, v]) => v !== undefined));
  return value;
}
it.each(read('android-path').map((v: { case: string }, i: number) => ({ ...v, name: `${v.case}/${i}` })))('$name: Android path golden', vector => {
  const v = vector as unknown as { input: PartialInput; expected: ConnectionPath };
  expect(canonical(presentPath(v.input, 'UTC'))).toEqual(canonical(v.expected));
});
it.each([...read('android-stage'), ...read('android-samples')].map((v: { case?: string }, i: number) => ({ ...v, name: `${v.case ?? 'SamplePaths'}/${i}` })))('$name: Android stage golden', vector => {
  const v = vector as unknown as { path: ConnectionPath; flags: object; expected: unknown };
  expect(canonical(presentStage(v.path, v.flags))).toEqual(canonical(v.expected));
});
it('rejects an action tap for 500 ms after the action changes', () => { expect(stageTapSettled(1000, 1499)).toBe(false); expect(stageTapSettled(1000, 1500)).toBe(true); });
it.each(read('linux'))('$case', vector => {
  const path = presentPath(vector.input, 'UTC');
  expect(path).toMatchObject(vector.expected);
  expect(presentStage(path)).toMatchObject(vector.stage);
  expect(path.sections.flatMap(s => s.steps.map(s => s.id))).not.toEqual(expect.arrayContaining(['KEEP_ALIVE', 'USB_HOST', 'DLNA', 'MIRACAST', 'TESLA']));
});
import { titles, explanations, actionLabels, failureExplanation } from '../src/path/texts';
import { countdownSeconds, formatDiagnostic } from '../src/path/time';
it.each(read('android-rules'))('$case: $kind $input', vector => {
  const input = vector.input;
  const actual = vector.kind === 'title' ? titles[input as keyof typeof titles] : vector.kind === 'explanation' ? explanations[input as keyof typeof explanations] : vector.kind === 'actionLabel' ? actionLabels[input as keyof typeof actionLabels] : vector.kind === 'failureExplanation' ? failureExplanation(input) : vector.kind === 'countdown' ? countdownSeconds(10000 + input, 10000) : stageTapSettled(1000, 1000 + input);
  expect(actual).toBe(vector.expected);
});
it('formats diagnostics with the relay timestamp in the selected time zone', () => { expect(formatDiagnostic('R MODE✓', '0.1.6', 1792852325000, 'UTC')).toBe('R MODE✓ | 0.1.6 14:32:05'); });
