// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-15

import { describe, expect, it } from 'vitest';
import { isVisible, controlFor, coerceValue, validatePassword, type Field } from '../src/schema';
const field = (type: Field['type'], extra: Partial<Field> = {}): Field => ({ key: 'test', labelKey: 'field.test', type, ...extra });
describe('schema decisions', () => {
  it('shows unconditional fields', () => expect(isVisible(field('text'), {})).toBe(true));
  it('matches any visibleWhen value', () => expect(isVisible(field('text', { visibleWhen: { key: 'mode', equals: ['a', 'b'] } }), { mode: 'b' })).toBe(true));
  it('hides mismatched or missing dependencies', () => {
    const conditional = field('text', { visibleWhen: { key: 'mode', equals: ['a'] } });
    expect(isVisible(conditional, { mode: 'b' })).toBe(false);
    expect(isVisible(conditional, {})).toBe(false);
  });
  it('matches boolean dependencies without string conversion', () => {
    const conditional = field('text', { visibleWhen: { key: 'enabled', equals: [true] } });
    expect(isVisible(conditional, { enabled: true })).toBe(true);
    expect(isVisible(conditional, { enabled: 'true' })).toBe(false);
  });
  it.each([['bool', 'switch'], ['choice', 'radio'], ['text', 'text'], ['secret', 'password'], ['number', 'number'], ['phone', 'picker']] as const)('maps %s to %s', (type, control) => expect(controlFor(field(type))).toBe(control));
  it('coerces switches safely', () => { expect(coerceValue(field('bool'), true)).toBe(true); expect(coerceValue(field('bool'), 'false')).toBe(false); });
  it('converts numeric input', () => expect(coerceValue(field('number'), '1280')).toBe(1280));
  it('keeps empty numeric values for Linux automatic channels', () => expect(coerceValue(field('number'), '')).toBe(''));
  it('does not serialize invalid numbers as null', () => expect(coerceValue(field('number'), 'oops')).toBe('oops'));
  it('preserves choice value types', () => expect(coerceValue(field('choice', { options: [{ value: 5, labelKey: 'five' }] }), '5')).toBe(5));
  it('preserves secret whitespace and phone identifiers', () => { expect(coerceValue(field('secret'), ' a ')).toBe(' a '); expect(coerceValue(field('phone'), 'auto')).toBe('auto'); });
});
describe('password validation', () => {
  it.each(['openxplay', 'a long password', '12345678', ' 密码密码密码密码 '])('accepts %s', password => expect(validatePassword(password, password)).toBeNull());
  it.each(['', 'short', '1234567'])('rejects short password %j', password => expect(validatePassword(password, password)).toBe('error.passwordFormat'));
  it('rejects a confirmation mismatch', () => expect(validatePassword('openxplay', 'different')).toBe('error.passwordMismatch'));
  it('preserves spaces when comparing passwords', () => expect(validatePassword(' password ', 'password')).toBe('error.passwordMismatch'));
});
