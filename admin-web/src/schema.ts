// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-15

export interface SavedNetwork { ssid: string; hasPassword?: boolean; current?: boolean; password?: string }
export interface PublicNameProvider { name: string; tokenSet: boolean }
export type PublicNameProviders = Record<string, PublicNameProvider>;
export type Value = string | number | boolean | null | SavedNetwork[] | string[] | PublicNameProviders;
export type Values = Record<string, Value>;
export type Illustration = 'wifi-direct' | 'phone-hotspot' | 'device-hotspot' | 'shared-wifi';
export interface Option { value: Value; labelKey: string; hintKey?: string; illustration?: Illustration }
export interface Field { key: string; type: 'bool' | 'choice' | 'text' | 'secret' | 'number' | 'phone' | 'networks' | 'plugins'; labelKey: string; hintKey?: string; options?: Option[]; visibleWhen?: { key: string; equals: Value[] }; min?: number; max?: number }
export interface Section { id: string; titleKey: string; fields: Field[] }
export interface InstalledPlugin { id: string; name: Record<string, string>; icon: string | null; version: string; license: string; nonCommercialAssets: boolean }
export interface Settings { schema: Section[]; values: Values; installedPlugins?: InstalledPlugin[] }
export function isVisible(field: Field, values: Values): boolean {
  return !field.visibleWhen || field.visibleWhen.equals.includes(values[field.visibleWhen.key]);
}
export function controlFor(field: Field): string {
  return ({ bool: 'switch', choice: 'radio', text: 'text', secret: 'password', number: 'number', phone: 'picker', networks: 'networks', plugins: 'plugins' })[field.type];
}
export function coerceValue(field: Field, raw: string | boolean): Value {
  if (field.type === 'bool') return raw === true || raw === 'true';
  if (field.type === 'number') {
    if (String(raw).trim() === '') return '';
    const number = Number(raw);
    return Number.isFinite(number) ? number : String(raw);
  }
  if (field.type === 'choice') return field.options?.find(option => String(option.value) === String(raw))?.value ?? String(raw);
  return String(raw);
}
export function validatePassword(password: string, confirm: string): string | null {
  if (password.length < 8) return 'error.passwordFormat';
  if (confirm !== password) return 'error.passwordMismatch';
  return null;
}
