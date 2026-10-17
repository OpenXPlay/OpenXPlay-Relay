// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-15

import { icons } from './lucide-icons';

export function icon(name: string): string { return (icons[name] ?? icons["circle-help"]).replace("<svg", '<svg aria-hidden="true" focusable="false"'); }
