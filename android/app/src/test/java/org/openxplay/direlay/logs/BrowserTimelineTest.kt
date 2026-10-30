// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-23
package org.openxplay.direlay.logs

import android.app.Application
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.openxplay.stream.core.*
import org.openxplay.stream.web.WebOutput
import org.openxplay.stream.web.WebOutputConfig
import java.net.Socket
import java.nio.file.Files
import java.util.concurrent.CopyOnWriteArrayList
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicInteger

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [33], manifest = Config.NONE, application = Application::class)
class BrowserTimelineTest {
    @Test fun reportsAreDeduplicatedAndNeverInventSocketLifecycle() {
        val lines = mutableListOf<String>()
        val log = ConnectionLog(clock = { 100 }, write = { _, line -> lines += line })
        val browser = BrowserTimeline(log)
        try {
            val info = """{"userAgent":"Chrome/123","screen":{"width":1280,"height":720},"videoDecoder":true,"videoDecode":[{"codec":"avc1.640020","width":1280,"height":720,"preferHardware":true}]}"""
            browser.report("car", "receiver-info", info); browser.report("car", "receiver-info", info)
            browser.report("car", "stats", """{"rendered":1}""")
            browser.report("car", "stats", """{"rendered":2}""")
            browser.report("car", "relay-close", "write_timeout")
            browser.awaitIdle(); log.awaitIdle()
            assertEquals(1, lines.count { it.startsWith("browser client_observed ") })
            assertEquals(1, lines.count { it.startsWith("browser receiver_info ") })
            assertEquals(1, lines.count { it.startsWith("browser first_render_observed ") })
            assertEquals(1, lines.count { it.startsWith("browser client_disconnected ") })
            assertTrue(lines.any { it.contains("ua=Chrome screen=1280x720") && it.contains("avc1.640020") })
            assertFalse(lines.any { it.startsWith("browser client_connected ") || it.contains("first_frame_delivered") })
        } finally { browser.close(); log.close() }
    }

    @Test fun everySocketEndCauseHasOneBoundedLinkEndAndTheReceiverViewStaysSeparate() {
        val lines = mutableListOf<String>()
        val log = ConnectionLog(clock = { 100 }, write = { _, line -> lines += line })
        val browser = BrowserTimeline(log)
        val causes = listOf("peer_close", "peer_eof", "peer_reset", "read_timeout", "protocol_error", "io_error", "queue_overflow", "write_timeout", "stopped", "replaced", "teardown")
        try {
            causes.forEachIndexed { index, cause ->
                val client = "$index"
                browser.report(client, "client-opened", """{"secure":true,"activeClients":1}""")
                browser.report(client, "client-opened", """{"secure":true,"activeClients":1}""")
                val closed = """{"by":"${if (cause.startsWith("peer")) "peer" else "relay"}","cause":"$cause","code":1001,"peerReason":"closed\npassword=private mac=AA:BB:CC:DD:EE:FF","durationMs":250,"rxIdleMs":10,"txStalledMs":20,"videoFramesSent":8,"droppedFrames":2,"bytesSent":1024}"""
                browser.report(client, "client-closed", closed)
                browser.report(client, "client-closed", closed)
            }
            browser.report("new", "receiver-close", """{"code":1006,"reason":"reload","wasClean":false,"openForMs":500,"sinceLastMessageMs":20,"cause":"reload","visibility":"visible","online":true,"reconnects":2}""")
            browser.awaitIdle(); log.awaitIdle()
            assertEquals(causes.size, lines.count { it.startsWith("browser client_opened ") })
            val ends = lines.filter { it.startsWith("browser link_ended ") }
            assertEquals(causes.size, ends.size)
            causes.forEach { cause -> assertEquals(1, ends.count { "cause=$cause " in it }) }
            assertTrue(ends.all { "link=web_client reached=opened" in it && "age=250 idle=10" in it && "videoFramesSent=8" in it })
            assertTrue(ends.filter { "cause=write_timeout" in it || "cause=read_timeout" in it }.all { "by=timeout" in it })
            assertTrue(ends.all { "private" !in it && "AA:BB:CC:DD:EE:FF" !in it && '\n' !in it })
            assertEquals(1, lines.count { it.startsWith("browser receiver_close ") && "cause=reload" in it && "openForMs=500" in it && "reconnects=2" in it })
        } finally { browser.close(); log.close() }
    }

    @Test fun lifecycleIsObservedBeforeTheHubCanDiscardADetachedOutputsReports() {
        val lines = mutableListOf<String>()
        val log = ConnectionLog(clock = { 100 }, write = { _, line -> lines += line })
        val browser = BrowserTimeline(log)
        lateinit var upstream: OutputHost
        val output = object : ProjectionOutput {
            override val id = "web"
            override fun open(host: OutputHost) { upstream = host }
            override fun onSession(state: SessionState, config: SessionConfig) = Unit
            override fun onVideoFormat(screen: ScreenId, format: VideoFormat?) = Unit
            override fun onVideoFrame(frame: VideoFrame) = Unit
            override fun onAudioFormat(stream: Int, format: AudioFormat?) = Unit
            override fun onAudioPacket(packet: AudioPacket) = Unit
            override fun onMicrophone(stream: Int, request: MicRequest?) = Unit
            override fun close() = Unit
        }
        try {
            TimelineWebOutput(output, browser).open(object : OutputHost {
                override fun capabilities(capabilities: OutputCapabilities) = Unit
                override fun control(event: ControlEvent) = Unit
                override fun stats(stats: OutputStats) = Unit
                // A detached hub intentionally ignores reports; the observer must already have them.
                override fun receiverDiagnostics(origin: Origin, type: String, json: String) = Unit
            })
            upstream.receiverDiagnostics(Origin("web", "1"), "client-opened", """{"secure":true,"activeClients":1}""")
            upstream.receiverDiagnostics(Origin("web", "1"), "client-closed", """{"by":"relay","cause":"stopped","durationMs":1}""")
            browser.awaitIdle(); log.awaitIdle()
            assertEquals(1, lines.count { it.startsWith("browser client_opened ") })
            assertEquals(1, lines.count { it.startsWith("browser link_ended ") && "by=local cause=stopped" in it })
        } finally { browser.close(); log.close() }
    }

    @Test fun stoppingAWebOutputLogsEveryOpenClientThroughTheWrapperAfterHubDetach() {
        val lines = CopyOnWriteArrayList<String>()
        val opened = CountDownLatch(2)
        val closed = CountDownLatch(2)
        val log = ConnectionLog(clock = { 100 }, write = { _, line ->
            lines += line
            if (line.startsWith("browser client_opened ")) opened.countDown()
            if (line.startsWith("browser link_ended ")) closed.countDown()
        })
        val browser = BrowserTimeline(log)
        val root = Files.createTempDirectory("browser-timeline")
        val web = WebOutput(WebOutputConfig(port = 0, staticAssetRoot = root, sourceUrl = "https://openxplay.org/source"))
        val hubClosed = AtomicInteger()
        val hub = ProjectionHub(FakeInput(), listener = object : HubListener {
            override fun receiverDiagnostics(outputId: String, origin: Origin, type: String, json: String) {
                if (type == "client-closed") hubClosed.incrementAndGet()
            }
        })
        val sockets = mutableListOf<Socket>()
        try {
            hub.addOutput(TimelineWebOutput(web, browser))
            val port = web.awaitListening()
            repeat(2) {
                val socket = Socket("127.0.0.1", port).also { sockets += it; it.soTimeout = 5_000 }
                val key = java.util.Base64.getEncoder().encodeToString(ByteArray(16) { n -> (n + 1).toByte() })
                socket.getOutputStream().apply {
                    write(("GET /oxp/v0 HTTP/1.1\r\nHost: localhost\r\nConnection: Upgrade\r\n" +
                        "Upgrade: websocket\r\nSec-WebSocket-Version: 13\r\nSec-WebSocket-Key: $key\r\n\r\n").toByteArray(Charsets.US_ASCII))
                    flush()
                }
                val response = socket.getInputStream().bufferedReader(Charsets.US_ASCII)
                assertTrue(requireNotNull(response.readLine()).startsWith("HTTP/1.1 101 "))
                while (requireNotNull(response.readLine()).isNotEmpty()) { }
            }
            assertTrue(opened.await(5, TimeUnit.SECONDS))
            hub.removeOutput("web")
            assertTrue(closed.await(5, TimeUnit.SECONDS))
            browser.awaitIdle(); log.awaitIdle()
            val opens = lines.filter { it.startsWith("browser client_opened ") }
            val ends = lines.filter { it.startsWith("browser link_ended ") }
            fun client(line: String) = line.substringAfter("client=").substringBefore(' ')
            assertEquals(2, opens.size)
            assertEquals(2, ends.size)
            assertEquals(opens.map(::client).toSet(), ends.map(::client).toSet())
            assertTrue(ends.all { "by=local cause=stopped" in it })
            assertEquals(0, hubClosed.get())
        } finally {
            hub.stop(); web.close()
            sockets.forEach { it.close() }
            browser.close(); log.close()
            root.toFile().deleteRecursively()
        }
    }
}
