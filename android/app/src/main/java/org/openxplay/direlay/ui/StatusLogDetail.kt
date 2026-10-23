// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-23
package org.openxplay.direlay.ui

/** Resource identifiers and state only; displayed arguments and device names never enter the log. */
fun statusLogDetail(headline: Int, subline: Int?, tone: StatusTone, value: HomeSnapshot,
    resourceEntryName: (Int) -> String): String {
    fun name(resource: Int): String = runCatching { resourceEntryName(resource) }.getOrElse { resource.toString() }
    return "headline=${name(headline)} subline=${subline?.let(::name) ?: "none"} tone=$tone " +
        "phase=${value.phase?.name ?: "none"} stage=${value.stage} started=${value.started} wireless=${value.wireless}"
}
