// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-16
package org.openxplay.direlay.plugins

import com.shilapi.xcertplay.orchestration.MfiTarget
import org.junit.Assert.*
import org.junit.Test
import org.openxplay.direlay.settings.ConnectionMode
import org.openxplay.direlay.settings.RelaySettings
import org.openxplay.stream.core.*
import org.openxplay.stream.web.*
import java.io.DataInputStream
import java.net.Socket
import java.nio.file.Files

class PluginsOffGoldenTest {
    private val settings = RelaySettings(ConnectionMode.USB, MfiTarget.I2C, "/dev/i2c-0", "", "", null, 60, false, false, false, null)
    private val host = object : OutputHost {
        override fun capabilities(capabilities: OutputCapabilities) = Unit
        override fun control(event: ControlEvent) = Unit
        override fun stats(stats: OutputStats) = Unit
    }
    private fun request(port: Int, method: String, path: String, upgrade: Boolean = false): ByteArray = Socket("127.0.0.1", port).use { socket ->
        socket.soTimeout = 3000
        val headers = if (upgrade) {
            "Connection: Upgrade\r\nUpgrade: websocket\r\nSec-WebSocket-Version: 13\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\n"
        } else "Connection: close\r\n"
        socket.getOutputStream().write("$method $path HTTP/1.1\r\nHost: localhost\r\n$headers\r\n".toByteArray())
        socket.getInputStream().readBytes().also { assertTrue(it.isNotEmpty()) }
    }
    private fun welcome(port: Int): String = Socket("127.0.0.1", port).use { socket ->
        socket.soTimeout = 3000
        val output = socket.getOutputStream()
        val input = DataInputStream(socket.getInputStream())
        output.write(("GET /oxp/v0 HTTP/1.1\r\nHost: localhost\r\nUpgrade: websocket\r\nConnection: Upgrade\r\n" +
            "Sec-WebSocket-Version: 13\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\n\r\n").toByteArray())
        val headers = StringBuilder()
        while (!headers.endsWith("\r\n\r\n")) headers.append(input.readUnsignedByte().toChar())
        assertTrue(headers.startsWith("HTTP/1.1 101"))
        val hello = """{"type":"hello","version":0,"token":"test","decoders":[{"codecString":"avc1.42E01F","hardware":false}],"displays":[{"screen":0,"width":1280,"height":720,"dpr":1,"refreshHz":60,"safeArea":{"top":0,"right":0,"bottom":0,"left":0}}],"maxTouchContacts":10,"audio":[],"microphone":false}""".toByteArray()
        output.write(0x81); output.write(0xFE)
        output.write(hello.size ushr 8); output.write(hello.size and 255)
        val mask = byteArrayOf(1, 2, 3, 4)
        output.write(mask)
        output.write(ByteArray(hello.size) { (hello[it].toInt() xor mask[it % 4].toInt()).toByte() })
        output.flush()
        assertEquals(0x81, input.readUnsignedByte())
        val short = input.readUnsignedByte()
        val size = if (short == 126) input.readUnsignedShort() else short
        val payload = ByteArray(size).also(input::readFully)
        String(payload, Charsets.UTF_8).replace(Regex("\"clientId\":\"[^\"]+\""), "\"clientId\":\"browser-1\"")
    }

    @Test fun offAndEmptySelectionPreserveHelloAndRoutesByteForByte() {
        val root = Files.createTempDirectory("relay-off").toFile()
        try {
            root.resolve("index.html").writeText("receiver-golden")
            val missing = root.resolve("packages-absent")
            for (value in listOf(settings, settings.copy(pluginsEnabled = listOf("org.test.clock")),
                settings.copy(plugins = true, pluginsEnabled = listOf("org.test.missing")))) {
                val config = RelayPlugins.configuration(value, missing, root.resolve("state"))
                assertNull(config)
                val web = WebOutput(WebOutputConfig(bindAddress = "127.0.0.1", port = 0, staticAssetRoot = root.toPath(),
                    sourceUrl = "https://github.com/OpenXPlay/OpenXPlay-Relay", plugins = config))
                assertFalse(web.wantsContext())
                web.open(host)
                try {
                    web.onSession(SessionState(SessionId(42), SessionPhase.ACTIVE), SessionConfig(emptyMap()))
                    val port = web.awaitListening()
                    val expected = """{"type":"welcome","version":0,"clientId":"browser-1","session":{"id":"42","phase":"ACTIVE"},"sourceUrl":"https://github.com/OpenXPlay/OpenXPlay-Relay","attribution":"Powered by OpenXPlay.","projectUrl":"https://github.com/OpenXPlay/OpenXPlay-Stream"}"""
                    assertArrayEquals(expected.toByteArray(), welcome(port).toByteArray())
                    assertTrue(String(request(port, "GET", "/index.html")).endsWith("\r\n\r\nreceiver-golden"))
                    for (method in listOf("GET", "HEAD", "POST")) for (tail in listOf(
                        "index.json", "host/index.html", "channel", "p/org.test.clock/index.html", "artwork/x",
                    )) {
                        assertArrayEquals("$method /plugins/$tail", request(port, method, "/zz-missing/$tail"), request(port, method, "/plugins/$tail"))
                    }
                    assertArrayEquals("WebSocket upgrade /plugins/channel", request(port, "GET", "/zz-missing/channel", true), request(port, "GET", "/plugins/channel", true))
                    assertFalse(missing.exists())
                    assertFalse(root.resolve("state").exists())
                } finally { web.close() }
            }
        } finally { root.deleteRecursively() }
    }
}
