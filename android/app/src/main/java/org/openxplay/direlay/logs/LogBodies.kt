// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-22
package org.openxplay.direlay.logs

import com.shilapi.xcertplay.airplay.BplistCodec
import java.nio.ByteBuffer
import java.nio.charset.CodingErrorAction

internal class LogBodies(private val redactor: LogRedactor) {
    fun decodeHex(hex: String): String {
        val size = (hex.length + 1) / 2
        if (hex.length % 2 != 0 || size > 1_048_576) return "<binary $size bytes>"
        return runCatching { decode(ByteArray(size) { hex.substring(it * 2, it * 2 + 2).toInt(16).toByte() }) }
            .getOrDefault("<binary $size bytes>")
    }
    private fun decode(bytes: ByteArray): String {
        if (bytes.isEmpty()) return "<binary 0 bytes>"
        if (bytes.take(8).toByteArray().contentEquals(magic)) return render(BplistCodec.decode(bytes))
        val end = separator(bytes)
        if (end != null) {
            val head = printable(bytes.copyOfRange(0, end.first))
            if (head != null && rtsp.containsMatchIn(head)) {
                // Decoded messages stay on one log line: line breaks are written as \r\n escapes.
                val cleaned = head.lineSequence().filter { it.isNotEmpty() }.joinToString("\\r\\n") { line ->
                    val colon = line.indexOf(':')
                    val key = if (colon > 0) line.substring(0, colon) else ""
                    if (LogRedactor.keyKind(key) == "secret") line.substring(0, colon + 1) + " " + redactor.privateValue(key, line.substring(colon + 1).trim())
                    else redactor.redact(line)
                }
                val body = bytes.copyOfRange(end.first + end.second, bytes.size)
                return cleaned + "\\r\\n\\r\\n" + if (body.isEmpty()) "" else decode(body)
            }
        }
        val text = printable(bytes) ?: return "<binary ${bytes.size} bytes>"
        return redactor.redact(text).replace("\r", "\\r").replace("\n", "\\n")
    }
    private fun printable(bytes: ByteArray): String? = runCatching {
        val text = Charsets.UTF_8.newDecoder().onMalformedInput(CodingErrorAction.REPORT)
            .onUnmappableCharacter(CodingErrorAction.REPORT).decode(ByteBuffer.wrap(bytes)).toString()
        text.takeIf { it.none { c -> c.isISOControl() && c !in "\r\n\t" } }
    }.getOrNull()
    private fun separator(bytes: ByteArray): Pair<Int, Int>? {
        for (i in 0 until bytes.size - 1) {
            if (bytes[i] == 10.toByte() && bytes[i + 1] == 10.toByte()) return i to 2
            if (i + 3 < bytes.size && bytes[i] == 13.toByte() && bytes[i + 1] == 10.toByte() &&
                bytes[i + 2] == 13.toByte() && bytes[i + 3] == 10.toByte()) return i to 4
        }
        return null
    }
    private fun render(value: Any?): String {
        val output = StringBuilder("bplist")
        var full = false
        fun append(text: String) {
            if (full) return
            if (output.length + text.length <= 2000) output.append(text) else { output.append('…'); full = true }
        }
        fun visit(item: Any?, depth: Int, key: String = "", phoneInfo: Boolean = false) {
            if (full) return
            val kind = LogRedactor.keyKind(key) ?: if (key == "name" && phoneInfo) "name" else null
            when {
                kind == "material" -> append(redactor.privateValue(key, item.toString(), kind))
                kind != null && item !is ByteArray ->
                    append(redactor.privateValue(key, item.toString(), kind))
                item is ByteArray -> append("<data ${item.size} bytes>")
                depth > 6 -> append("<depth limit>")
                item is Map<*, *> -> {
                    val identity = item.keys.any { it.toString().equals("deviceID", true) || it == "model" }
                    append("{")
                    item.entries.forEachIndexed { index, entry ->
                        if (index > 0) append(", ")
                        append(entry.key.toString() + "=")
                        visit(entry.value, depth + 1, entry.key.toString(), identity)
                    }
                    append("}")
                }
                item is List<*> -> {
                    append("[")
                    item.forEachIndexed { index, entry -> if (index > 0) append(", "); visit(entry, depth + 1, key, phoneInfo) }
                    append("]")
                }
                item is String -> append(redactor.redact(item).replace("\r", "\\r").replace("\n", "\\n"))
                else -> append(item.toString())
            }
        }
        visit(value, 0)
        return output.toString()
    }
    companion object {
        private val magic = "bplist00".toByteArray(Charsets.US_ASCII)
        private val rtsp = Regex("^(?:RTSP/|HTTP/|[A-Z]+ (?:[A-Za-z][A-Za-z0-9+.-]*://|/|\\*))")
    }
}
