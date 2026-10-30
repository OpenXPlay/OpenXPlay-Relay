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
  expect(presentStage(path, vector.flags)).toMatchObject(vector.stage);
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

import { normalizeInput } from '../src/path/model';
import { stageCommand } from '../src/path/actions';
import { stepText } from '../src/stage';
import { translate } from '../src/i18n';
it('omits Linux permissions and their radio prerequisite edges', () => {
  const path = presentPath({ platform: 'linux', setup: { valid: false, network: 'PHONE_HOTSPOT' }, relay: { hotspotOn: false } });
  const steps = path.sections.flatMap(s => s.steps);
  expect(steps.some(s => s.id === 'PERMISSIONS')).toBe(false);
  expect(steps.find(s => s.id === 'HOTSPOT')?.state).toBe('ACTION');
  expect(presentPath({ platform: 'android' }).sections.flatMap(s => s.steps).some(s => s.id === 'PERMISSIONS')).toBe(true);
});
it('pauses Linux armed startup through the existing stop operation', () => {
  expect(stageCommand({ type: 'PauseAutomatic' }, 'linux', true)).toEqual({ kind: 'api', endpoint: 'actions/stop' });
  expect(stageCommand({ type: 'PauseAutomatic' }, 'android', true)).toBeNull();
});
it('keeps the Linux choose-car notice quiet while running and preserves Android notices', () => {
  const input = normalizeInput({ platform: 'linux', iphone: { started: true } });
  expect(presentStage(presentPath(input), { carIncomplete: true }).notice).toBeNull();
  expect(presentStage(presentPath({ ...input, platform: 'android' }), { carIncomplete: true }).notice).toBe('CHOOSE_CAR');
  expect(presentStage(presentPath({ ...input, iphone: { ...input.iphone, started: false } }), { carIncomplete: true }).notice).toBe('CHOOSE_CAR');
});
it.each(['en', 'zh-CN'] as const)('localizes the armed blocker and checking-iPhone headline (%s)', language => {
  const t = (key: string, args?: Record<string, string | number>) => translate(language, key, args);
  expect(stepText({ res: 'path_start_when', args: ['path_car_page'] }, t)).toContain(t('path_car_page'));
  const path = presentPath({ platform: 'linux', setup: { network: 'USB' }, relay: { identity: 'READY' }, iphone: { usb: 'HOST', started: true, step: 'AUTH' } });
  expect(path.sections.flatMap(s => s.steps).some(s => s.id === 'PERMISSIONS')).toBe(false);
  const checking = presentPath({ ...read('linux')[0].input, iphone: { pick: 'PICKED', started: true, step: 'AUTH' } });
  expect(presentStage(checking).headline).toMatchObject({ res: 'path_checking_iphone' });
});
it('stops the elapsed counter on a disconnected Carlink output', () => {
  const path = presentPath({ platform: 'linux', setup: { outputs: ['CARLINK'] }, relay: { identity: 'READY', bluetoothOn: true, wifiOn: true }, car: { carlink: { phase: 'ENDED', trusted: true, sinceMillis: 100 } }, iphone: { pick: 'PICKED', started: true, step: 'VIDEO' } });
  expect(presentStage(path).headline).toMatchObject({ res: 'path_carlink_disconnected' });
  expect(presentStage(path).supporting).toBeNull();
});

it('keeps an active retry ahead of the armed automatic-start wait', () => {
  const input = read('linux').find((v: { case: string }) => v.case === 'Linux iPhone failure with automatic retry').input;
  const view = presentStage(presentPath({ ...input, iphone: { ...input.iphone, armed: true } }));
  expect(view.headline).toMatchObject({ res: 'path_trying_again' });
  expect(view.primary).toBeNull(); expect(view.secondary).toEqual({ type: 'Stop' });
});
