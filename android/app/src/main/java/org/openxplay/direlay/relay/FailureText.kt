// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-21
package org.openxplay.direlay.relay

import java.util.Collections
import java.util.IdentityHashMap

/** Keep the original messages and protocol codes, without retaining the Throwable or its secrets. */
internal fun failureText(failure: Throwable, redact: (String) -> String = { redactFailureText(it) }): String {
    val seen = Collections.newSetFromMap(IdentityHashMap<Throwable, Boolean>())
    val lines = mutableListOf<String>()
    var current: Throwable? = failure
    while (current != null && lines.size < 5 && seen.add(current)) {
        val text = current.message?.takeIf { it.isNotBlank() }
            ?: current.javaClass.simpleName.ifEmpty { current.javaClass.name }
        lines.add(if (lines.isEmpty()) text else "Caused by: $text")
        current = current.cause
    }
    return redact(lines.joinToString("\n"))
}

private val credentialValue = Regex(
    """(?i)(\b(?:remote[_ -]?(?:mfi[_ -]?)?token|access[_ -]?token|token|password|passphrase|passcode|pin|psk)\b["']?\s*(?:[:=]|\bis\b)\s*)("[^"]*"|'[^']*'|[^\s,;}&]+)""")
private val urlCredentials = Regex("""(?i)([a-z][a-z0-9+.-]*://)[^\s/@]+@""")
private val bearerToken = Regex("""(?i)(\bBearer\s+)[^\s,;]+""")

/** Also redact known values when an upstream exception includes a credential without a label. */
internal fun redactFailureText(text: String, secrets: Collection<String> = emptyList()): String {
    val known = secrets.filter { it.isNotEmpty() }.distinct().sortedByDescending { it.length }
    val redacted = known.fold(text) { value, secret -> value.replace(secret, "[redacted]") }
    return bearerToken.replace(urlCredentials.replace(credentialValue.replace(redacted) {
        "${it.groupValues[1]}[redacted]"
    }) { "${it.groupValues[1]}[redacted]@" }) { "${it.groupValues[1]}[redacted]" }
}
