// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-22
package org.openxplay.direlay.logs

import java.util.Locale

class LogRedactor(val secrets: LogSecrets = LogSecrets()) {
    private val bodies = LogBodies(this)
    fun redact(line: String): String {
        secrets.refresh()
        val header = LogLine.header(line)
        val message = line.substring(header.length)
        val lower = message.lowercase(Locale.ROOT).replace("_", "").replace("-", "")
        if (triggers.none { lower.contains(it) } && !message.any { it == '+' || it == '@' } &&
            !chinesePhone.containsMatchIn(message) && !secrets.mightContain(message)) return line
        var text = airplayHex.replace(message) { match -> match.groupValues[1] + bodies.decodeHex(match.groupValues[2]) }
        text = redactNames(text)
        text = secrets.redact(text)
        text = plist.replace(text) { match ->
            val kind = keyKind(match.groupValues[2])
            if (kind == null) match.value else match.groupValues[1] + privateValue(match.groupValues[2], match.groupValues[3], kind) + match.groupValues[4]
        }
        val replacements = mutableListOf<Pair<IntRange, String>>()
        var through = -1
        keyed.findAll(text).forEach { match ->
            val key = match.groupValues[1]
            val kind = keyKind(key)
            if (match.range.first > through && kind != null) {
                val start = match.range.last + 1
                val cookie = suffixes(key).any { it == "cookie" || it == "setcookie" } &&
                    text.getOrNull(start) !in listOf('"', '\'') && LogTokens.marker.find(text, start)?.range?.first != start
                val found = typedValue.find(text, start)?.takeIf { it.range.first == start }
                    ?: escapedQuoted.find(text, start)?.takeIf { it.range.first == start }
                    ?: (if (cookie) cookieValue.find(text, start) else null)
                    ?: bearer.find(text, start)?.takeIf { it.range.first == start } ?: value.find(text, start)
                if (found != null && found.range.first == start) {
                    val range = collectionValue(text, start) ?: found.range
                    val raw = text.substring(range)
                    val bare = unquote(raw)
                    val replacement = when {
                        kind in personalKinds && raw.startsWith("bytes=") -> bytesPreview.replace(raw) { it.groupValues[1] + "[hidden]" }
                        kind in personalKinds && scalar.containsMatchIn(raw) -> raw
                        typedValue.matches(raw) && kind in setOf("secret", "material") -> "[secret]"
                        bearer.matches(raw) -> redactBearer(raw)
                        else -> privateValue(key, bare, kind)
                    }
                    replacements += range to when {
                        raw.startsWith('"') -> "\"$replacement\""
                        raw.startsWith('\'') -> "'$replacement'"
                        else -> replacement
                    }
                    through = range.last
                }
            }
        }
        for ((range, replacement) in replacements.asReversed()) text = text.replaceRange(range, replacement)
        text = redactBearer(text)
        text = userInfo.replace(text) { it.groupValues[1] + secrets.tokens.token(it.groupValues[2]) + "@" }
        if (privateKeyStart.containsMatchIn(text)) return header + "[secret]"
        text = redactNmea(text)
        text = sensorPreviews(text)
        return header + secrets.redact(redactContacts(text))
    }
    private fun collectionValue(text: String, start: Int): IntRange? {
        if (text[start] !in "[{" || LogTokens.marker.find(text, start)?.range?.first == start) return null
        var depth = 0
        var quote: Char? = null
        var escaped = false
        for (index in start until text.length) {
            val c = text[index]
            when {
                escaped -> escaped = false
                c == '\\' -> escaped = true
                quote != null -> if (c == quote) quote = null
                c == '"' || c == '\'' -> quote = c
                c == '[' || c == '{' -> depth++
                c == ']' || c == '}' -> if (--depth == 0) return start..index
                c == '\n' || c == '\r' -> return start until index
            }
        }
        return start until text.length
    }
    internal fun privateValue(key: String, value: String, kind: String = keyKind(key) ?: "text"): String {
        if (LogTokens.isPublic(value)) return value
        learn(key, value, kind)
        return when (kind) {
            "location" -> "[location]"
            "material" -> "[secret]"
            else -> secrets.tokens.token(value, kind)
        }
    }
    private fun learn(key: String, value: String, kind: String) {
        val minimum = if (kind == "secret" && suffixes(key).any { it in learnedKeys }) 8 else if (kind in personalKinds) 4 else return
        // Keep the quote/escape-stripped form for unusually escaped formatter values.
        val bare = value.trim('\\', '"', '\'')
        secrets.remember(listOf(bare, value).distinct().filter { it.length >= minimum }, kind = kind)
    }
    internal fun personalStrings(message: String): String = strings.replace(message) { match ->
        val original = unquote(match.value)
        if (bundle.matches(original) || LogTokens.isPublic(original)) match.value else {
            val key = field.find(message)?.groupValues?.get(1).orEmpty()
            val kind = when {
                keyKind(key) in setOf("secret", "material", "name", "phone", "email") -> keyKind(key)!!
                email.matches(original) -> "email"
                phoneNumber.matches(original) || chinesePhone.matches(original) -> "phone"
                else -> "text"
            }
            "\"${privateValue(key, original, kind)}\""
        }
    }.let { bytesPreview.replace(it) { match -> match.groupValues[1] + "[hidden]" } }
    internal fun redactContacts(text: String) = LogTokens.outsideMarkers(text) { marked -> outsideHex(marked) { part ->
        var result = email.replace(part) { secrets.tokens.token(it.value, "email") }
        result = phoneNumber.replace(result) { secrets.tokens.token(it.value, "phone") }
        chinesePhone.replace(result) { secrets.tokens.token(it.value, "phone") }
    } }
    private fun outsideHex(text: String, replace: (String) -> String): String = buildString {
        var start = 0
        for (match in hexPreview.findAll(text)) {
            append(replace(text.substring(start, match.range.first))); append(match.value)
            start = match.range.last + 1
        }
        append(replace(text.substring(start)))
    }
    private fun redactBearer(text: String) = bearer.replace(text) { it.groupValues[1] + " " + secrets.tokens.token(it.groupValues[2]) }
    private fun redactNames(text: String): String {
        var result = phoneName.replace(text) { it.groupValues[1] + privateValue("deviceName", unquote(it.groupValues[2]), "name") }
        if (result.contains("Multiple connected iPhones found: ")) result = listedPhoneName.replace(result) {
            it.groupValues[1] + privateValue("deviceName", it.groupValues[2], "name") + it.groupValues[3]
        }
        if (result.contains("airplay /info request=", true) && infoIdentity.containsMatchIn(result)) result = infoName.replace(result) {
            it.groupValues[1] + privateValue("deviceName", unquote(it.groupValues[2]), "name")
        }
        return result
    }
    private fun redactNmea(text: String) = nmea.replace(text) { sentence ->
        var found = false
        val cleaned = coordinate.replace(sentence.value) { found = true; ",[location]," + it.groupValues[1] }
        if (found || sentence.value.contains("[location]")) cleaned else "[location]"
    }
    private fun sensorPreviews(text: String): String {
        if (!sensorService.containsMatchIn(text)) return text
        return traceBytes.replace(text) { match ->
            val length = traceLength.find(match.value)?.groupValues?.get(1)
            tracePreview.replace(match.value) { it.groupValues[1] + (length?.let { size -> "[hidden $size bytes]" } ?: "[hidden]") + it.groupValues[3] }
        }
    }
    companion object {
        private val credentialKeys = setOf("password", "passwd", "pwd", "passphrase", "psk", "presharedkey", "secret", "token",
            "accesstoken", "remotetoken", "apikey", "authorization", "cookie", "setcookie", "pin", "pincode", "passcode", "setupcode", "pairingcode")
        private val materialKeys = setOf("privatekey", "publickey", "pk8", "sessionkey", "readkey", "writekey", "shk", "ekey", "eiv",
            "pk", "salt", "proof", "signature", "certificate", "srp", "x25519")
        private val locationKeys = setOf("latitude", "longitude", "lat", "lon", "lng")
        private val personalKeys = mapOf("devicename" to "name", "displayname" to "name", "contactname" to "name", "callername" to "name",
            "remoteid" to "phone", "phonenumber" to "phone", "email" to "email")
        private val personalKinds = setOf("name", "phone", "email", "text")
        private val learnedKeys = setOf("passphrase", "password", "psk", "presharedkey")
        private val camel = Regex("(?<=[a-z0-9])(?=[A-Z])|(?<=[A-Z])(?=[A-Z][a-z])|[_-]+")
        private fun suffixes(key: String): List<String> {
            val parts = camel.split(key).map { it.lowercase(Locale.ROOT) }
            return parts.indices.map { parts.drop(it).joinToString("") }
        }
        internal fun keyKind(key: String): String? {
            val suffixes = suffixes(key)
            return when {
                suffixes.any { it in credentialKeys } -> "secret"
                suffixes.any { it in materialKeys } -> "material"
                suffixes.any { it in locationKeys } -> "location"
                else -> suffixes.firstNotNullOfOrNull { personalKeys[it] }
            }
        }
        private val triggers = credentialKeys + materialKeys + locationKeys + personalKeys.keys +
            listOf("hex", "begin", "bearer", "basic", "://", "\$", "name=", "iphones found:", "sensor", "/info")
        private val keyed = Regex("(?<![\\w-])([A-Za-z_][A-Za-z0-9_-]*)[\"']?\\s*(?:=|:|\\bis\\b)\\s*", RegexOption.IGNORE_CASE)
        private val value = Regex("${LogTokens.marker.pattern}|<empty>|\"(?:\\\\.|[^\"\\\\])*\"?|'(?:\\\\.|[^'\\\\])*'?|\\[\\]|\\{\\}|[^\\s,;&)\\]\\}>]+")
        private val strings = Regex("\"(?:\\\\.|[^\"\\\\])*\"")
        private val escapedQuoted = Regex("""\\(["'])(?:\\.|[^\\\r\n])*?\\\1""")
        private val cookieValue = Regex("[^\\r\\n\\}]+?(?=,\\s*[A-Za-z][A-Za-z0-9_-]*[\"']?\\s*[=:]|[\\r\\n\\}]|$)")
        private val plist = Regex("(<key>([^<]+)</key>\\s*<(?:string|integer|data|real)>)(.*?)(</(?:string|integer|data|real)>)", RegexOption.IGNORE_CASE)
        private val airplayHex = Regex("((?<![\\w-])(?:bodyHex|wireHex|bplistHex|raw\\d+Hex)[\"']?\\s*[=:]\\s*)[\"']?([0-9a-f]+)[\"']?(?![\\w])", RegexOption.IGNORE_CASE)
        private val bearer = Regex("\\b(Bearer|Basic)\\s+(${LogTokens.marker.pattern}|[^\\s,;\"'<>]+)", RegexOption.IGNORE_CASE)
        private val userInfo = Regex("([A-Za-z][A-Za-z0-9+.-]*://)([^\\s/@]+)@")
        private val phoneNumber = Regex("(?<![\\p{L}\\p{N}_+./-])\\+(?:[ \\t(]*[0-9][ \\t()\\-]*){6,14}[0-9](?![\\p{L}\\p{N}_+:/\\-]|\\.[\\p{L}\\p{N}]|[ \\t()\\-]*[0-9])")
        private val chinesePhone = Regex("(?<![\\p{L}\\p{N}_+./\\-])1[3-9][0-9]{9}(?![\\p{L}\\p{N}_+:/\\-]|\\.[\\p{L}\\p{N}])")
        private val email = Regex("(?<![\\w.+-])[A-Za-z0-9_!#$%&'*+/^`\\{|\\}~.-]+@[A-Za-z0-9](?:[A-Za-z0-9.-]*[A-Za-z0-9])?\\.[A-Za-z]{2,}(?![\\w-]|\\.[\\w])")
        private val hexPreview = Regex("(?<![\\w-])(?:[A-Za-z0-9_-]*hex|raw-body)[\"']?\\s*[=:]\\s*[\"']?(?:[0-9a-f]+(?:[ \\t]+[0-9a-f]{2})*)(?![\\w])", RegexOption.IGNORE_CASE)
        private val bundle = Regex("[A-Za-z][A-Za-z0-9_-]*(?:\\.[A-Za-z][A-Za-z0-9_-]*){2,}")
        private val nmea = Regex("\\${'$'}[A-Za-z]{2}[A-Za-z]{3},[^\\s\"'<>]*")
        private val coordinate = Regex(",(?:[0-9]{4,5}\\.[0-9]+),([NSEW])(?=,|\\*|$)")
        private val phoneName = Regex("((?:AirPlay device info|Bluetooth target) name=)(.*?)(?= [A-Za-z][A-Za-z0-9]*=|$)")
        private val listedPhoneName = Regex("((?:Multiple connected iPhones found: |, ))(.*?)( \\([0-9a-fA-F]{2}(?::[0-9a-fA-F]{2}){5}\\))")
        private val infoIdentity = Regex("\\b(?:deviceID|deviceId|model)=")
        private val infoName = Regex("(\\bname=)(.*?)(?=, [A-Za-z][A-Za-z0-9]*=|\\}|$)")
        private val sensorService = Regex("[\"']?service[\"']?\\s*[:=]\\s*[\"']sensor[\"']")
        private val traceBytes = Regex("\\{\\s*(?:\"length\"\\s*:\\s*\\d+\\s*,\\s*)?\"preview\"\\s*:\\s*\"(?:\\\\.|[^\"\\\\])*(?:\"(?:\\s*,\\s*\"length\"\\s*:\\s*\\d+)?\\s*\\}|$)")
        private val traceLength = Regex("\"length\"\\s*:\\s*(\\d+)")
        private val tracePreview = Regex("(\"preview\"\\s*:\\s*\")((?:\\\\.|[^\"\\\\])*)(\"|$)")
        private val scalar = Regex("^(?:[ui](?:8|16|32|64)=|bool(?:=|$))")
        private val typedValue = Regex("bytes=\\d+ \\[[^]\\r\\n]*]|[ui](?:8|16|32|64)=[^\\r\\n]+|void\\? [^\\r\\n]+")
        private val field = Regex("^\\s+\\[\\d+] ([A-Za-z0-9_-]+):")
        private val bytesPreview = Regex("(bytes=\\d+ )\\[[^]]*]")
        internal val privateKeyStart = Regex("-----BEGIN ([A-Z ]*PRIVATE KEY)-----")
        private val escapes = Regex("\\\\(u[0-9a-fA-F]{4}|[\\\\\"'/nrtbf])")
        internal fun unquote(value: String): String {
            if (value.length >= 4 && value.startsWith('\\') && value[1] in "\"'" && value.endsWith("\\" + value[1]))
                return unquote(value.substring(1, value.length - 2) + value[1])
            if (!value.startsWith('"') && !value.startsWith('\'')) return value
            val text = value.drop(1).dropLast(if (value.lastOrNull() == value.first()) 1 else 0)
            return escapes.replace(text) { when (val escaped = it.groupValues[1]) {
                "n" -> "\n"; "r" -> "\r"; "t" -> "\t"; "b" -> "\b"; "f" -> "\u000c"
                else -> if (escaped.startsWith('u')) escaped.drop(1).toInt(16).toChar().toString() else escaped
            } }
        }
    }
}
