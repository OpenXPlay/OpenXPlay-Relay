// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-22
package org.openxplay.direlay.settings

data class DlnaRendererChoice(val udn: String, val name: String)
enum class DlnaPicture(val width: Int, val height: Int) { P720(1280, 720), P1080(1920, 1080) }
