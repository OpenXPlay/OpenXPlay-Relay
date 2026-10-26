// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-22
package org.openxplay.direlay.logs

import com.shilapi.xcertplay.airplay.BplistCodec
import com.shilapi.xcertplay.iap2.message.Iap2CarPlayMessages
import com.shilapi.xcertplay.iap2.message.Iap2ControlMessages
import com.shilapi.xcertplay.iap2.message.Iap2Messages
import com.shilapi.xcertplay.iap2.message.Iap2WirelessMessages
import com.shilapi.xcertplay.iap2.message.Iap2WirelessSessionParameters
import com.shilapi.xcertplay.iap2.trace.Iap2FrameFormatter
import com.shilapi.xcertplay.iap2.trace.Iap2TraceDirection
import com.shilapi.xcertplay.iap2.wire.Iap2Frame
import org.junit.Assert.*
import org.junit.Test
import java.net.URLEncoder
import java.util.Base64

class LogRedactorTest {
    private val key = ByteArray(32) { it.toByte() }
    private val tokens = LogTokens(key)
    private fun redactor() = LogRedactor(LogSecrets(key = key))
    private val redactor = redactor()
    private fun token(value: String, kind: String = "secret") = tokens.token(value, kind)
    private fun header(time: String = "04:57:30.180") = "2026-10-23 $time +0000  2485  2726 I xcertplay-usb: "
    private fun hex(bytes: ByteArray) = bytes.joinToString("") { "%02x".format(it.toInt() and 255) }
    private fun formatted(frame: Iap2Frame, time: String = "04:57:30.180"): List<String> =
        Iap2FrameFormatter.format(Iap2TraceDirection.RX, "usb", frame).lines().map { header(time) + it }

    @Test fun tokensAreStableAcrossInstancesAndDifferentForDifferentValuesAndKeys() {
        val first = redactor.redact("password=alpha")
        assertTrue(first.matches(Regex("password=\\[secret#[0-9a-f]{6}]")))
        assertEquals(first, redactor().redact("password=alpha"))
        assertNotEquals(first, redactor.redact("password=bravo"))
        assertNotEquals(first, LogRedactor(LogSecrets(key = ByteArray(32) { 42 })).redact("password=alpha"))
        assertEquals(token("alpha").substringAfter('#'), token("alpha", "name").substringAfter('#'))
    }
    @Test fun everySensitiveKeyFormKeepsTheKeyAndTokenizesOnlyTheValue() {
        val samples = mapOf(
            "password=x" to "password=${token("x")}", "passwd: abc" to "passwd: ${token("abc")}", "pwd = 1" to "pwd = ${token("1")}",
            "password is xyz" to "password is ${token("xyz")}", "\"token\":\"abc\"" to "\"token\":\"${token("abc")}\"",
            "\"pin\" : 123" to "\"pin\" : ${token("123")}", "[1] passphrase: \"abc def\"" to "[1] passphrase: \"${token("abc def")}\"",
            "<key>password</key><string>abc def</string>" to "<key>password</key><string>${token("abc def")}</string>",
            "WirelessNetworkSettings(manualPassphrase=hunter2!x)" to "WirelessNetworkSettings(manualPassphrase=${token("hunter2!x")})",
            "nested: password=abc, other=1" to "nested: password=${token("abc")}, other=1")
        samples.forEach { (input, expected) -> assertEquals(input, expected, redactor.redact(input)) }
    }
    @Test fun onlySuffixSegmentsMatchAndQualifiersRemainDiagnostic() {
        for (key in listOf("hotspotPassphrase", "wifi_password", "remoteMfiToken", "pinCode", "manualPassphrase",
            "pre_shared_key", "api-key", "set-cookie", "PASSWORD", "carlinkPin", "requirePairingCode")) {
            assertEquals(key, "$key=${token("short")}", redactor.redact("$key=short"))
        }
        for (line in listOf("passphraseLength=12", "pinRequired=true", "tokenExpiresAt=2026-10-23T04:57:30Z", "hasPasswordSet=false",
            "mapping=1", "ping 8.8.8.8", "shipping=ready", "keyframe=true", "keyboard=usb", "tokenizer=ok", "pinned=true",
            "latency=12", "along=road", "ssid=MyWiFi", "password_hash_length=64", "api-key-version=2")) {
            assertEquals(line, line, redactor.redact(line))
        }
    }
    @Test fun publicValuesAndExistingMarkersArePreservedUnderEverySensitiveClass() {
        for (key in listOf("password", "privateKey", "deviceName", "remoteId", "latitude", "Cookie")) {
            for (value in listOf("", "true", "false", "null", "nil", "none", "empty", "<empty>", "\"\"", "''", "[]", "{}",
                "[secret]", "[location]", "[hidden 24 bytes]", "[redacted]", token("original"), token("song", "text"))) {
                val line = "$key=$value"
                assertEquals(line, line, redactor.redact(line))
            }
        }
    }
    @Test fun credentialsLocationAndPersonalKeysHaveTheirOwnClasses() {
        for (key in listOf("password", "passwd", "pwd", "passphrase", "psk", "presharedkey", "secret", "token", "accesstoken",
            "remotetoken", "apikey", "authorization", "cookie", "setcookie", "pin", "pincode", "passcode", "setupcode", "pairingcode")) {
            assertEquals(key, "$key=${token("q")}", redactor.redact("$key=q"))
        }
        for (key in listOf("private_key", "pk8", "sessionkey", "readkey", "writekey", "shk", "ekey", "eiv", "pk", "salt", "proof", "signature", "certificate"))
            assertEquals(key, "$key=[secret]", redactor.redact("$key=q"))
        for (key in listOf("latitude", "longitude", "lat", "lon", "lng"))
            assertEquals(key, "$key=[location]", redactor.redact("$key=31.12345"))
        for (key in listOf("deviceName", "displayName", "contactName", "callerName"))
            assertEquals(key, "$key=${token("Alice", "name")}", redactor().redact("$key=Alice"))
        assertEquals("remoteId=${token("12345", "phone")}", redactor.redact("remoteId=12345"))
        assertEquals("phoneNumber=${token("+14155551234", "phone")}", redactor.redact("phoneNumber=+14155551234"))
        assertEquals("email=${token("a@example.org", "email")}", redactor.redact("email=a@example.org"))
    }
    @Test fun knownValuesCoverPlainJsonAndUrlFormsWithTheSameToken() {
        val value = "abc\"\\/ def"
        val log = LogRedactor(LogSecrets(key = key).apply { remember(listOf(value, "tiny", "123456")) })
        for (input in listOf(value, "abc\\\"\\\\/ def", URLEncoder.encode(value, "UTF-8"), URLEncoder.encode(value, "UTF-8").replace("+", "%20")))
            assertEquals(input, token(value), log.redact(input))
        assertEquals(token(value), log.redact(URLEncoder.encode(value, "UTF-8").lowercase()))
        assertEquals("tiny 1234567 a123456 123456b _123456", log.redact("tiny 1234567 a123456 123456b _123456"))
        assertEquals("(${token("123456")}), ${token("123456")}", log.redact("(123456), 123456"))
    }
    @Test fun learningKeepsCredentialThresholdAndShortPersonalWordBoundaries() {
        for (key in listOf("passphrase", "password", "psk", "preSharedKey")) {
            val log = redactor()
            log.redact("$key=learned-passphrase")
            assertEquals("later ${token("learned-passphrase")}", log.redact("later learned-passphrase"))
        }
        val log = redactor()
        log.redact("password=short")
        assertEquals("later short", log.redact("later short"))
        log.redact("displayName=Alice")
        assertEquals("PHONE ${token("Alice", "name")} AliceMusic", log.redact("PHONE Alice AliceMusic"))
    }
    @Test fun learnsBareValueFromEscapedQuotes() {
        val log = redactor()
        assertFalse(log.redact("passphrase: \\\"Escaped2024x\\\"").contains("Escaped2024x"))
        assertEquals("later ${token("Escaped2024x")}", log.redact("later Escaped2024x"))
        val spaced = redactor()
        assertEquals("passphrase: ${token("Escaped 2024x")}", spaced.redact("passphrase: \\\"Escaped 2024x\\\""))
        assertEquals("later ${token("Escaped 2024x")}", spaced.redact("later Escaped 2024x"))
    }
    @Test fun sensitiveCollectionsDoNotLeakTheirSecondOrNestedValues() {
        for (value in listOf("[\"private one\",\"private two\"]", "{\"a\":\"private one\",\"b\":[\"private two\"]}")) {
            val output = redactor.redact("password=$value next=ok")
            assertEquals("password=${token(value)} next=ok", output)
            assertEquals(output, redactor.redact(output))
        }
    }
    @Test fun airplayBplistPreservesStructureAndHidesAllKeyAndDataRepresentations() {
        val material = "NeverShowKey123!".toByteArray()
        val body = BplistCodec.encode(linkedMapOf("shk" to material, "ekey" to material.decodeToString(), "eiv" to material,
            "streams" to listOf(mapOf("type" to 110, "sampleRate" to 48000, "active" to true)),
            "payload" to byteArrayOf(1, 2, 3), "label" to "diagnostic"))
        for (key in listOf("bodyHex", "wireHex", "bplistHex", "raw123Hex")) {
            val output = redactor.redact("$key=${hex(body)} next=ok")
            assertTrue(output.contains("$key=bplist{")); assertTrue(output.contains("shk=[secret]")); assertTrue(output.contains("ekey=[secret]"))
            assertTrue(output.contains("type=110")); assertTrue(output.contains("sampleRate=48000")); assertTrue(output.contains("active=true"))
            assertTrue(output.contains("payload=<data 3 bytes>")); assertTrue(output.contains("label=diagnostic")); assertTrue(output.endsWith("next=ok"))
            for (form in listOf(material.decodeToString(), hex(material), Base64.getEncoder().encodeToString(material), material.contentToString()))
                assertFalse(form, output.contains(form))
            assertEquals(output, redactor.redact(output))
        }
    }
    @Test fun rtspHeadAndBplistBodyAreDecodedWithOnlyPrivateHeaderValuesHidden() {
        val body = BplistCodec.encode(mapOf("shk" to byteArrayOf(1, 2, 3), "ekey" to "AudioKeyMaterial", "port" to 7000))
        val head = "SETUP rtsp://192.168.1.3/stream RTSP/1.0\r\nCSeq: 3\r\nAuthorization: Basic c2VjcmV0\r\nCookie: session=private; other=abc\r\nContent-Type: application/x-apple-binary-plist\r\n\r\n"
        val output = redactor.redact("wireHex=" + hex(head.toByteArray() + body))
        assertTrue(output.contains("SETUP rtsp://192.168.1.3/stream RTSP/1.0")); assertTrue(output.contains("CSeq: 3"))
        assertFalse(output.contains('\n')); assertFalse(output.contains('\r')); assertTrue(output.contains("RTSP/1.0\\r\\nCSeq: 3"))
        assertTrue(output.contains("Content-Type: application/x-apple-binary-plist")); assertTrue(output.contains("port=7000"))
        for (secret in listOf("c2VjcmV0", "session=private", "other=abc", "AudioKeyMaterial")) assertFalse(output.contains(secret))
        assertEquals(output, redactor.redact(output))
    }
    @Test fun airplayHeaderMapsKeepHeadersWithoutLeakingAdditionalCookies() {
        val cookie = "session=private; other=also-private"
        val output = redactor.redact("TRACE airplay control rx headers={Cookie=$cookie, CSeq=3, Content-Type=application/octet-stream} bodyHex=0102")
        assertEquals("TRACE airplay control rx headers={Cookie=${token(cookie)}, CSeq=3, Content-Type=application/octet-stream} bodyHex=<binary 2 bytes>", output)
        assertEquals(output, redactor.redact(output))
    }
    @Test fun printableBodiesSurviveAndBinaryOrInvalidBplistBodiesHaveOnlySize() {
        assertEquals("bodyHex=volume=0.5 model=iPhone16,2", redactor.redact("bodyHex=" + hex("volume=0.5 model=iPhone16,2".toByteArray())))
        for (key in listOf("bodyHex", "wireHex", "bplistHex", "raw69Hex"))
            assertEquals("$key=<binary 4 bytes> next=ok", redactor.redact("$key=0102abff next=ok"))
        assertEquals("bodyHex=<binary 8 bytes>", redactor.redact("bodyHex=" + hex("bplist00".toByteArray())))
        assertEquals("bodyHex=<binary 2 bytes>", redactor.redact("bodyHex=c328"))
        assertEquals("bodyHex=<binary 2 bytes>", redactor.redact("bodyHex=abc"))
    }
    @Test fun plistPhoneNamesAndNestedValuesAreRedactedWithBoundedOutput() {
        val body = BplistCodec.encode(mapOf("deviceID" to "AA:BB:CC:DD:EE:FF", "model" to "iPhone16,2", "name" to "Alice's iPhone",
            "nested" to listOf(mapOf("latitude" to 31.123, "email" to "alice@example.org", "count" to 4))))
        val output = redactor.redact("bplistHex=" + hex(body))
        assertTrue(output.contains("name=${token("Alice's iPhone", "name")}")); assertTrue(output.contains("model=iPhone16,2"))
        assertTrue(output.contains("AA:BB:CC:DD:EE:FF")); assertTrue(output.contains("latitude=[location]")); assertTrue(output.contains("count=4"))
        assertFalse(output.contains("alice@example.org"))
        var nested: Any = "deep"
        repeat(10) { nested = listOf(nested) }
        assertTrue(redactor.redact("bodyHex=" + hex(BplistCodec.encode(nested))).contains("<depth limit>"))
        val large = redactor.redact("bodyHex=" + hex(BplistCodec.encode(mapOf("label" to "a".repeat(3000)))))
        assertTrue(large.length <= "bodyHex=".length + 2001); assertTrue(large.endsWith('…'))
        val truncated = redactor.redact("bodyHex=" + hex(BplistCodec.encode(linkedMapOf("label" to "a".repeat(1970), "password" to "private"))))
        assertEquals(truncated, redactor.redact(truncated))
    }
    @Test fun realWirelessFormatterFramesKeepSsidNumericFieldsAndNestingButHidePassphrasesAndRawBodies() {
        val password = "wireless-passphrase"
        val frames = listOf(
            Iap2CarPlayMessages.startSession(7000, "public-key-data", "1", wireless = Iap2WirelessSessionParameters("MyWiFi", password, 6, listOf("192.168.1.1"), 1)),
            Iap2WirelessMessages.accessoryWiFiConfiguration("MyWiFi", password, 6, 1),
            Iap2Messages.buildRaw(0x5701) { u8(0, 0); string(2, "MyWiFi"); string(3, password) })
        val stream = LogRedactorStream(redactor)
        for (frame in frames) {
            val input = formatted(frame)
            val output = input.map(stream::redact).joinToString("\n")
            assertTrue(output.contains("MyWiFi")); assertTrue(output.contains("passphrase: \"${token(password)}\""))
            assertTrue(output.contains("raw-body=[hidden ${frame.payload.size} bytes]")); assertFalse(output.contains(password))
            if (frame.messageId == 0x4301) { assertTrue(output.contains("wireless: {")); assertTrue(output.contains("u32=7000")); assertTrue(output.contains("192.168.1.1")) }
            assertTrue(output.contains("u8="))
        }
        assertEquals("later ${token(password)}", stream.redact("later $password"))
    }
    @Test fun realPersonalFramesKeepLabelsAndNumericValuesWhileLearningStringsForPhoneLines() {
        val caller = "+14155551234"
        for (id in listOf(0x4155, 0x4158, 0x5001, 0x5003, 0x5201, 0x5202, 0x5204, 0x4171, 0x415a, 0x4c04)) {
            val frame = Iap2Messages.buildRaw(id) {
                string(0, caller); string(1, "Private contact and song"); u16(2, 513); u8(3, 7)
                string(4, "com.apple.Music"); bytes(5, byteArrayOf(1, 2, 3, 4, 5)); string(6, "alice@example.org")
            }
            val input = formatted(frame)
            val stream = LogRedactorStream(redactor())
            val output = input.map(stream::redact)
            assertEquals(input.first(), output.first())
            val joined = output.joinToString("\n")
            assertTrue(joined.contains("[0] param: \"${token(caller, "phone")}\""))
            assertTrue(joined.contains(token("Private contact and song", "text"))); assertTrue(joined.contains("len=24"))
            assertTrue(joined.contains("u16=513 (0x02 01)")); assertTrue(joined.contains("u8=7 (0x07)"))
            assertTrue(joined.contains("com.apple.Music")); assertTrue(joined.contains("bytes=5 [hidden]"))
            assertFalse(joined.contains("Private contact and song")); assertFalse(joined.contains(caller)); assertFalse(joined.contains("alice@example.org"))
            val phone = header("04:57:30.181") + "PHONE call $caller connected status=2"
            assertEquals(header("04:57:30.181") + "PHONE call ${token(caller, "phone")} connected status=2", stream.redact(phone))
            assertEquals("PHONE ${token("Private contact and song", "text")}", stream.redact("PHONE Private contact and song"))
        }
    }
    @Test fun personalEntryKeepsNestingAndEnumAndBooleanDiagnostics() {
        val stream = LogRedactorStream(redactor)
        stream.redact(header() + "IAP2 RX [usb] 0x5001 NowPlayingUpdate frame=12B body=1 params")
        for (line in listOf("  [0] param: {", "    [1] status: bool=true", "    [2] state: PLAYING", "  }"))
            assertEquals(header() + line, stream.redact(header() + line))
        assertEquals(header() + "    [3] title: \"${token("Secret song", "text")}\"", stream.redact(header() + "    [3] title: \"Secret song\""))
    }
    @Test fun rawBodiesOfNonPrivateFramesStayByteIdentical() {
        val stream = LogRedactorStream(redactor)
        val input = formatted(Iap2Messages.buildRaw(0x4e0b) { u32(0, 12) })
        assertEquals(input, input.map(stream::redact))
        assertEquals("raw-body=0a 1b ... (+10B)", redactor.redact("raw-body=0a 1b ... (+10B)"))
    }
    @Test fun realLocationFramesKeepNmeaTimeFixSpeedAndHemisphereButLoseCoordinates() {
        val sentence = "${'$'}GPRMC,123519,A,4807.038,N,01131.000,E,022.4,084.4,230394,003.1,W*6A"
        val expected = "${'$'}GPRMC,123519,A,[location],N,[location],E,022.4,084.4,230394,003.1,W*6A"
        val lines = formatted(Iap2ControlMessages.locationInformation(sentence))
        val output = lines.map(LogRedactorStream(redactor)::redact).joinToString("\n")
        assertTrue(output.contains("raw-body=")); assertTrue(output.contains("24 47 50 52 4d 43"))
        assertFalse(output.contains("2c 34 38 30"))
        assertEquals(output, output.lines().map(LogRedactorStream(redactor)::redact).joinToString("\n"))
        assertTrue(output.contains("0xfffb")); assertTrue(output.contains(expected)); assertFalse(output.contains("4807.038")); assertFalse(output.contains("01131.000"))
        assertEquals(expected, redactor.redact(sentence))
        val gga = "${'$'}GNGGA,123519,4807.038,N,01131.000,E,1,08,0.9,545.4,M,46.9,M,,*47"
        assertEquals("${'$'}GNGGA,123519,[location],N,[location],E,1,08,0.9,545.4,M,46.9,M,,*47", redactor.redact(gga))
        assertEquals("${'$'}BDGGA,123519,[location],N,[location],E,1,08*47", redactor.redact("${'$'}BDGGA,123519,4807.038,N,01131.000,E,1,08*47"))
        assertEquals("position [location] end", redactor.redact("position ${'$'}GPRMC,broken end"))
    }
    @Test fun phoneAndEmailPatternsKeepSurroundingDiagnosticsAndAvoidTechnicalValues() {
        for (phone in listOf("+14155551234", "+86 138-0013-8000", "+1 (415) 555-1234", "13800138000", "+1234567", "+123456789012345"))
            assertEquals(phone, "call (${token(phone, "phone")}) status=2", redactor.redact("call ($phone) status=2"))
        assertEquals("call:${token("+14155551234", "phone")}", redactor.redact("call:+14155551234"))
        assertEquals("call:${token("13800138000", "phone")}", redactor.redact("call:13800138000"))
        assertEquals("Call ${token("+14155551234", "phone")}.", redactor.redact("Call +14155551234."))
        assertEquals("Mail ${token("a@example.org", "email")}.", redactor.redact("Mail a@example.org."))
        assertEquals("to=${token("a.b+tag@example.co.uk", "email")} status=ok", redactor.redact("to=a.b+tag@example.co.uk status=ok"))
        for (line in listOf(header(), "time=2026-10-22T12:30:45.123+0000", "ip=192.168.1.3:7000", "mac=aa:bb:cc:dd:ee:ff",
            "version=1.13800138000.2", "hex=ab13800138000cd", "hex=13800138000", "hex=0x13800138000", "raw-body=13 80 01 38 00 00", "counter=1234567890123456",
            "+123456", "+1234567890123456", "a13800138000b", "12345678901", "build=20261023045730"))
            assertEquals(line, line, redactor.redact(line))
    }
    @Test fun phoneLinesKeepEventsWithCredentialsAndLearnedPersonalValuesTokenized() {
        redactor.redact("callerName=Alice")
        val line = "PHONE call Alice +14155551234 account=alice@example.org status=2 password=topsecret"
        assertEquals("PHONE call ${token("Alice", "name")} ${token("+14155551234", "phone")} account=${token("alice@example.org", "email")} status=2 password=${token("topsecret")}", redactor.redact(line))
    }
    @Test fun phoneNameLogFormatsAreCoveredWithoutMaskingModelsOrRelayNames() {
        val name = "Alice's iPhone"
        for (prefix in listOf("AirPlay device info", "wireless selected Bluetooth target")) {
            val line = "$prefix name=$name deviceId=AA:BB:CC:DD:EE:FF model=iPhone16,2"
            assertEquals("$prefix name=${token(name, "name")} deviceId=AA:BB:CC:DD:EE:FF model=iPhone16,2", redactor().redact(line))
        }
        assertEquals("Multiple connected iPhones found: ${token(name, "name")} (AA:BB:CC:DD:EE:FF)",
            redactor.redact("Multiple connected iPhones found: $name (AA:BB:CC:DD:EE:FF)"))
        val info = redactor().redact("airplay /info request={name=$name, deviceID=AA:BB:CC:DD:EE:FF, model=iPhone16,2}")
        assertTrue(info.contains("name=${token(name, "name")}")); assertTrue(info.contains("model=iPhone16,2"))
        val input = formatted(Iap2Messages.buildRaw(0x4e09) { string(0, name) })
        val output = input.map(LogRedactorStream(redactor())::redact).joinToString("\n")
        assertTrue(output.contains("deviceName: \"${token(name, "name")}\"")); assertTrue(output.contains("raw-body=[hidden"))
        val clean = "relay name=OpenXPlay model=HeadUnit bluetoothAddress=AA:BB:CC:DD:EE:FF"
        assertEquals(clean, redactor.redact(clean))
    }
    @Test fun carlinkTraceHidesOnlySensorBytePreviewsRegardlessOfJsonKeyOrder() {
        for (fields in listOf("\"length\":12,\"preview\":\"private-gps\"", "\"preview\":\"private-gps\",\"length\":12")) {
            val line = "OXP-CarlinkTrace: {\"service\":\"sensor\",\"fields\":{\"1\":{$fields},\"2\":7}}"
            val output = redactor.redact(line)
            assertFalse(output.contains("private-gps")); assertTrue(output.contains("[hidden 12 bytes]")); assertTrue(output.contains("\"2\":7"))
            assertEquals(output, redactor.redact(output))
            for (service in listOf("control", "media", "uibc")) {
                val clean = line.replace("sensor", service)
                assertEquals(clean, redactor.redact(clean))
            }
        }
    }
    @Test fun typedSensitiveFieldsNeverLeaveTheirByteOrScalarPreviews() {
        for (line in listOf("  [0] privateKey: bytes=5 [01 02 03 04 05]", "  [0] privateKey: u32=1234 (0x00 00 04 d2)")) {
            val output = redactor.redact(line)
            assertTrue(output.endsWith("privateKey: [secret]")); assertFalse(output.contains("01 02")); assertFalse(output.contains("04 d2"))
        }
        assertEquals("  [0] remoteId: bytes=5 [hidden]", redactor.redact("  [0] remoteId: bytes=5 [01 02 03 04 05]"))
        assertEquals("key=hasEnabledAudio", redactor.redact("key=hasEnabledAudio"))
    }
    @Test fun truncatedSensorMirrorStillHidesTheLastPreview() {
        val prefix = "OXP-CarlinkTrace: {\"service\":\"sensor\",\"fields\":{\"1\":{\"length\":64,\"preview\":\""
        val input = prefix + "GPS {private coordinates"
        val output = redactor.redact(input)
        assertEquals(prefix + "[hidden 64 bytes]", output)
        assertEquals(output, redactor.redact(output))
    }
    @Test fun privateKeyBlocksHideEveryLineAndPreserveHeaders() {
        val stream = LogRedactorStream(redactor)
        for (message in listOf("-----BEGIN RSA PRIVATE KEY-----", "very private material", "-----END EC PRIVATE KEY-----", "still private", "-----END RSA PRIVATE KEY-----"))
            assertEquals(header() + "[secret]", stream.redact(header() + message))
        assertEquals(header() + "normal", stream.redact(header() + "normal"))
    }
    @Test fun bearerBasicAndUrlUserInfoBecomeStableTokens() {
        assertEquals("Bearer ${token("abc123")} Basic ${token("YWJj")}", redactor.redact("Bearer abc123 Basic YWJj"))
        assertEquals("https://${token("user:pass")}@example.org/path", redactor.redact("https://user:pass@example.org/path"))
        assertEquals("Authorization: Bearer ${token("topsecret")}", redactor.redact("Authorization: Bearer topsecret"))
    }
    @Test fun headerChangesEndPersonalEntriesIncludingPidTidTagAndTime() {
        for (next in listOf(header("04:57:30.181"), header().replace("2485", "2486"), header().replace("2726", "2727"), header().replace("xcertplay-usb", "Other"))) {
            val stream = LogRedactorStream(redactor)
            stream.redact(header() + "IAP2 RX [usb] 0x5001 NowPlayingUpdate frame=12B body=1 params")
            assertEquals(next + "[0] param: \"diagnostic\"", stream.redact(next + "[0] param: \"diagnostic\""))
        }
    }
    @Test fun redactionIsIdempotentAndUntriggeredLinesAreByteIdentical() {
        val examples = listOf("normal Ω diagnostic 123", "mapping=1 keyframe=true", "password=abc", "bodyHex=0102", "raw-body=0a 1b …",
            "Bearer secret", "https://u:p@host/", "PHONE private", "${'$'}GPRMC,123,456", "pinCode=1", "deviceName=Alice",
            "email=a@example.org", "+14155551234", "latitude=31.123", "[redacted 20 bytes]", "song=${token("private", "text")}")
        for (line in examples) { val once = redactor.redact(line); assertEquals(line, once, redactor.redact(once)) }
        val input = formatted(Iap2Messages.buildRaw(0x4155) { string(0, "private text"); u8(1, 3) })
        val once = input.map(LogRedactorStream(redactor)::redact)
        assertEquals(once, once.map(LogRedactorStream(redactor)::redact))
        val clean = header() + "normal Ω diagnostic 123"
        assertEquals(clean, redactor.redact(clean))
    }
    @Test fun shortRuntimePinsRemainTokenizedAfterTheHookRetires() {
        val log = LogRedactor(LogSecrets(key = key).apply { remember(listOf("1234"), allowShort = true) })
        assertEquals("PIN result ${token("1234")}", log.redact("PIN result 1234"))
        assertEquals(header(), log.redact(header()))
    }
    @Test fun hooksProduceStableTokensWithoutMatchingEmbeddedNumericPins() {
        var active = true
        val secrets = LogSecrets(hook = { if (active) it.replace("1234", "[redacted]").replace("hook-passphrase", "[redacted]") else it }, key = key)
        val log = LogRedactor(secrets)
        assertEquals("PIN ${token("1234")} PSK ${token("hook-passphrase")}", log.redact("PIN 1234 PSK hook-passphrase"))
        assertEquals("counter=123456 a1234 1234b", log.redact("counter=123456 a1234 1234b"))
        active = false
        assertEquals("PIN ${token("1234")} PSK ${token("hook-passphrase")}", log.redact("PIN 1234 PSK hook-passphrase"))
    }
    @Test fun knownValuesCannotModifyExistingMarkers() {
        val log = LogRedactor(LogSecrets(key = key).apply { remember(listOf("secret", "redacted", "location")) })
        for (marker in listOf("[redacted]", "[secret]", "[location]", token("value"), token("name", "name"), token("text", "text")))
            assertEquals(marker, log.redact(marker))
    }
}
