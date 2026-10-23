// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-22
package org.openxplay.direlay.logs

import com.shilapi.xcertplay.iap2.catalog.Iap2Direction
import com.shilapi.xcertplay.iap2.catalog.Iap2Endpoints
import com.shilapi.xcertplay.iap2.catalog.Iap2FieldSpec

class LogRedactorStream(private val redactor: LogRedactor) {
    private var privateKeyEnd: String? = null
    private var entry: String? = null
    private var personal = false
    private var hideBody = false
    private var location = false
    fun redact(line: String): String {
        val header = LogLine.header(line)
        var message = line.substring(header.length)
        if (privateKeyEnd == null) LogRedactor.privateKeyStart.find(message)?.let { privateKeyEnd = "-----END ${it.groupValues[1]}-----" }
        val end = privateKeyEnd
        if (end != null) {
            if (message.contains(end)) privateKeyEnd = null
            return header + "[secret]"
        }
        if (entry != null && entry != header) { entry = null; personal = false; hideBody = false; location = false }
        val frame = framePattern.find(message)
        if (frame != null) {
            val id = frame.groupValues[2].toInt(16)
            val endpoint = Iap2Endpoints.byId(id)
            val phoneNames = frame.groupValues[1] == "RX" && endpoint?.direction != Iap2Direction.ACCESSORY_TO_IPHONE &&
                endpoint?.fields?.let { fields -> contains(fields) { it.name == "deviceName" || it.name == "name" } } == true
            personal = id in personalIds || phoneNames
            location = id == 0xfff0 || id in 0xfffa..0xfffc
            hideBody = personal || endpoint?.fields?.let { fields -> contains(fields) { LogRedactor.keyKind(it.name) in setOf("secret", "material") } } == true
            entry = header
        } else if (entry != null) {
            if (hideBody) message = rawBody.replace(message) { match ->
                val preview = match.groupValues[2]
                if (preview.startsWith("[hidden")) match.value else {
                    val bytes = bytePair.findAll(preview.substringBefore("...").substringBefore('…')).count()
                    val remainder = omitted.find(preview)?.groupValues?.get(1)?.toIntOrNull() ?: 0
                    match.groupValues[1] + "[hidden ${bytes + remainder} bytes]"
                }
            }
            if (location) message = redactLocationPreview(message)
            if (personal) message = redactor.personalStrings(message)
        }
        return redactor.redact(header + message)
    }
    private fun redactLocationPreview(message: String) = rawBody.replace(message) { match ->
        val preview = match.groupValues[2]
        if (preview.contains("[location]")) match.value else {
            val pairs = bytePair.findAll(preview.substringBefore("...").substringBefore('…')).toList()
            val decoded = pairs.map { it.value.toInt(16).toChar() }.joinToString("")
            val ranges = mutableListOf<IntRange>()
            previewNmea.findAll(decoded).forEach { sentence ->
                val fields = sentence.value.split(',')
                // The GGA/RMC field positions come from DiPlay's NmeaLocationEncoder.
                val coordinates = when (fields.first().takeLast(3)) { "GGA" -> setOf(2, 4); "RMC" -> setOf(3, 5); else -> null }
                if (coordinates == null) ranges += sentence.range else {
                    var start = sentence.range.first
                    fields.forEachIndexed { index, field ->
                        if (index in coordinates && field.isNotEmpty()) ranges += start until start + field.length
                        start += field.length + 1
                    }
                }
            }
            var cleaned = preview
            for (range in ranges.asReversed()) cleaned = cleaned.replaceRange(pairs[range.first].range.first, pairs[range.last].range.last + 1, "[location]")
            match.groupValues[1] + cleaned
        }
    }
    companion object {
        private fun contains(fields: List<Iap2FieldSpec>, predicate: (Iap2FieldSpec) -> Boolean): Boolean =
            fields.any { predicate(it) || contains(it.children, predicate) }
        private val framePattern = Regex("^IAP2 (TX|RX) \\[[^]]*] 0x([0-9a-fA-F]{4})\\b")
        private val rawBody = Regex("(\\braw-body=)(.*)$")
        private val bytePair = Regex("[0-9a-fA-F]{2}")
        private val omitted = Regex("\\(\\+(\\d+)B\\)")
        private val previewNmea = Regex("\\${'$'}[A-Za-z]{2}[A-Za-z]{3},[^\\x00\\r\\n]*")
        internal val personalIds = (0x4154..0x4161).toSet() + (0x4170..0x4172) + (0x5000..0x5003) +
            (0x5200..0x5204) + (0x4c00..0x4c09) + setOf(0x4e09)
    }
}
