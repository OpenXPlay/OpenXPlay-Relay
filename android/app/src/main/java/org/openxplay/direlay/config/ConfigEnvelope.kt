// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-22
package org.openxplay.direlay.config

import org.json.JSONObject
import java.nio.ByteBuffer
import java.nio.charset.CodingErrorAction

class ConfigMeta(val appVersion: String, val exportedAt: String = java.time.Instant.now().toString())
internal class ConfigFailure(val key: String, val code: String) : IllegalArgumentException("$key: $code")
internal object ConfigEnvelope {
    const val MAX_BYTES = 64 * 1024
    fun create(format: String, meta: ConfigMeta) = JSONObject().put("format", format).put("version", 1)
        .put("platform", "android").put("appVersion", meta.appVersion).put("exportedAt", meta.exportedAt)
    fun text(bytes: ByteArray): String {
        if (bytes.size > MAX_BYTES) throw ConfigFailure("file", "too-large")
        return try { Charsets.UTF_8.newDecoder().onMalformedInput(CodingErrorAction.REPORT)
            .onUnmappableCharacter(CodingErrorAction.REPORT).decode(ByteBuffer.wrap(bytes)).toString() }
        catch (_: Exception) { throw ConfigFailure("file", "not-json") }
    }
    fun read(text: String, format: String): JSONObject {
        if (text.toByteArray(Charsets.UTF_8).size > MAX_BYTES) throw ConfigFailure("file", "too-large")
        val json = try { StrictJson(text).check(); JSONObject(text) }
        catch (_: Exception) { throw ConfigFailure("file", "not-json") }
        if (json.opt("format") != format) throw ConfigFailure("format",
            if (json.opt("format") in listOf(UserConfigCodec.FORMAT, PrivacyConfigCodec.FORMAT)) "wrong-kind" else "invalid")
        val version = json.opt("version")
        if (version !is Number || version.toDouble() != 1.0) throw ConfigFailure("version", "bad-version")
        return json
    }
    fun finish(json: JSONObject): String = json.toString(2).also {
        if (it.toByteArray(Charsets.UTF_8).size > MAX_BYTES) throw ConfigFailure("file", "too-large")
    }
}

/** org.json accepts non-JSON syntax; check the grammar before handing it any input. */
private class StrictJson(private val text: String) {
    private var at = 0
    private fun space() { while (at < text.length && text[at] in " \r\n\t") at++ }
    private fun take(c: Char): Boolean { space(); return (at < text.length && text[at] == c).also { if (it) at++ } }
    fun check() { value(0); space(); require(at == text.length) }
    private fun string(): String {
        space(); val start = at; require(take('"'))
        while (at < text.length) {
            val c = text[at++]
            if (c == '"') return text.substring(start, at)
            require(c >= ' ')
            if (c == '\\') {
                require(at < text.length)
                when (text[at++]) {
                    '"', '\\', '/', 'b', 'f', 'n', 'r', 't' -> Unit
                    'u' -> { require(at + 4 <= text.length && text.substring(at, at + 4).all { it in "0123456789abcdefABCDEF" }); at += 4 }
                    else -> error("escape")
                }
            }
        }
        error("string")
    }
    private fun value(depth: Int) {
        require(depth < 64); space(); require(at < text.length)
        when (text[at]) {
            '{' -> {
                at++; if (take('}')) return
                val keys = mutableSetOf<String>()
                do { require(keys.add(org.json.JSONTokener(string()).nextValue() as String)); require(take(':')); value(depth + 1) } while (take(','))
                require(take('}'))
            }
            '[' -> { at++; if (take(']')) return; do { value(depth + 1) } while (take(',')); require(take(']')) }
            '"' -> string()
            else -> {
                val match = Regex("(?:true|false|null|-?(?:0|[1-9][0-9]*)(?:\\.[0-9]+)?(?:[eE][+-]?[0-9]+)?)").find(text, at)
                require(match != null && match.range.first == at); at = match.range.last + 1
            }
        }
    }
}
internal fun JSONObject.string(key: String): String = (get(key) as? String) ?: throw ConfigFailure(key, "invalid")
internal fun JSONObject.bool(key: String): Boolean = (get(key) as? Boolean) ?: throw ConfigFailure(key, "invalid")
internal fun JSONObject.integer(key: String): Int {
    val value = get(key)
    require(value is Number && value.toDouble().isFinite() && value.toDouble() == value.toInt().toDouble())
    return value.toInt()
}
internal fun JSONObject.nullableInt(key: String) = if (isNull(key)) null else integer(key)
internal fun JSONObject.nullableString(key: String) = if (isNull(key)) null else string(key)
