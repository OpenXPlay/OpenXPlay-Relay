// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-15
package org.openxplay.direlay.relay

import org.openxplay.stream.web.TokenVerifier
import java.security.MessageDigest

fun webTokenVerifier(require: Boolean, code: String): TokenVerifier = if (!require) TokenVerifier { true }
    else TokenVerifier { candidate -> MessageDigest.isEqual(code.toByteArray(Charsets.UTF_8), candidate.toByteArray(Charsets.UTF_8)) }

fun receiverUrl(base: String, require: Boolean, code: String): String =
    base.substringBefore('#') + if (require) "#k=$code" else ""
fun visiblePairingCode(require: Boolean, code: String): String = if (require) code else ""
