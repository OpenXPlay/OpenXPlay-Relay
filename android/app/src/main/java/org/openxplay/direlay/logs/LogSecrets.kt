// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-22
package org.openxplay.direlay.logs

import java.net.URLEncoder
import java.util.Locale

class LogSecrets(private val refresh: (LogSecrets) -> Unit = {}, private val hook: (String) -> String = { it },
    key: ByteArray = LogTokens.randomKey()) {
    internal val tokens = LogTokens(key)
    private data class Value(val form: String, val replacement: String, val boundary: Regex?)
    private val values = linkedMapOf<String, Value>()
    @Volatile private var patterns: List<Value> = emptyList()
    fun refresh() = refresh(this)
    @Synchronized fun remember(secrets: Collection<String>, allowShort: Boolean = false, kind: String = "secret") {
        var changed = false
        secrets.filter { !LogTokens.isPublic(it) && (allowShort || it.length >= if (kind == "secret") 6 else 4) }.forEach { value ->
            val escaped = json(value, ascii = false)
            val ascii = json(value, ascii = true)
            val encoded = URLEncoder.encode(value, "UTF-8")
            val encodedLower = percent.replace(encoded) { it.value.lowercase(Locale.ROOT) }
            val replacement = tokens.token(value, kind)
            for (form in listOf(value, escaped, escaped.replace("/", "\\/"), ascii, ascii.replace("/", "\\/"),
                encoded, encoded.replace("+", "%20"), encodedLower, encodedLower.replace("+", "%20"))) {
                if (form !in values) {
                    val bounded = value.all(Char::isDigit) || (kind != "secret" && value.length < 8)
                    val boundary = if (bounded) Regex("(?<![\\p{L}\\p{N}_])" + Regex.escape(form) + "(?![\\p{L}\\p{N}_])") else null
                    values[form] = Value(form, replacement, boundary); changed = true
                }
            }
        }
        if (changed) patterns = values.values.sortedByDescending { it.form.length }
    }
    private fun json(value: String, ascii: Boolean): String = buildString {
        value.forEach { c -> append(when (c) {
            '\\' -> "\\\\"; '"' -> "\\\""; '\n' -> "\\n"; '\r' -> "\\r"; '\t' -> "\\t"; '\b' -> "\\b"; '\u000c' -> "\\f"
            else -> if (c < ' ' || (ascii && c > '~')) "\\u%04x".format(c.code) else c.toString()
        }) }
    }
    fun mightContain(text: String) = patterns.any { text.contains(it.form) } || hook(text) != text
    fun redact(text: String): String {
        val result = replaceKnown(text)
        if (hook(result) == result) return result
        return LogTokens.outsideMarkers(result) { part ->
            val hidden = hook(part)
            if (hidden == part) part else {
                // Existing PIN/PSK hooks replace each value with this marker; recover only those spans.
                val pieces = hidden.split("[redacted]")
                val match = Regex(pieces.joinToString("(.*?)") { Regex.escape(it) }, RegexOption.DOT_MATCHES_ALL).matchEntire(part)
                if (match == null || pieces.size < 2) hidden else {
                    remember(match.groupValues.drop(1), allowShort = true)
                    replaceKnown(part)
                }
            }
        }
    }
    private fun replaceKnown(text: String): String {
        var result = text
        for ((value, replacement, pattern) in patterns) if (result.contains(value)) result = LogTokens.outsideMarkers(result) { part ->
            pattern?.replace(part) { replacement } ?: part.replace(value, replacement)
        }
        return result
    }
    override fun toString() = "LogSecrets([secret])"
    companion object { private val percent = Regex("%[0-9A-F]{2}") }
}
