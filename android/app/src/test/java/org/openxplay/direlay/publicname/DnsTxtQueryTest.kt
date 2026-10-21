// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-20
package org.openxplay.direlay.publicname

import org.junit.Assert.*
import org.junit.Test
import java.io.ByteArrayOutputStream
import java.net.DatagramPacket
import java.net.DatagramSocket
import java.net.InetAddress
import java.net.InetSocketAddress
import java.net.UnknownHostException

class DnsTxtQueryTest {
    private val name = "_acme-challenge.myrelay.duckdns.org"

    private fun hex(text: String) = text.replace(" ", "").chunked(2).map { it.toInt(16).toByte() }.toByteArray()

    /** A reply to [id] that repeats the question and carries [records], each a list of character strings. */
    private fun reply(id: Int, records: List<List<String>>, rcode: Int = 0, truncated: Boolean = false, cut: Int = 0): ByteArray {
        val out = ByteArrayOutputStream()
        fun u16(value: Int) { out.write(value shr 8 and 255); out.write(value and 255) }
        u16(id); u16(0x8000 or 0x0400 or (if (truncated) 0x0200 else 0) or rcode); u16(1); u16(records.size); u16(0); u16(0)
        out.write(DnsMessage.query(0, name).copyOfRange(12, DnsMessage.query(0, name).size))
        records.forEach { strings ->
            u16(0xC00C) // A pointer to the question's name.
            u16(DnsMessage.TYPE_TXT); u16(1); u16(0); u16(60)
            val data = strings.flatMap { listOf(it.length.toByte()) + it.toByteArray().toList() }
            u16(data.size); out.write(data.toByteArray())
        }
        val bytes = out.toByteArray()
        return bytes.copyOf(bytes.size - cut)
    }

    @Test fun queryAsksTheServerDirectlyForTxtWithoutRecursion() {
        assertArrayEquals(hex("1234 0000 0001 0000 0000 0000" +
            "0f 5f61636d652d6368616c6c656e6765 07 6d7972656c6179 07 6475636b646e73 03 6f7267 00 0010 0001"),
            DnsMessage.query(0x1234, name))
        assertArrayEquals(DnsMessage.query(7, name), DnsMessage.query(7, "$name."))
        assertThrows(IllegalArgumentException::class.java) { DnsMessage.query(1, "a..b") }
        assertThrows(IllegalArgumentException::class.java) { DnsMessage.query(1, "x".repeat(64) + ".org") }
    }

    @Test fun answersJoinTheStringsOfEachRecord() {
        val parsed = checkNotNull(DnsMessage.parse(reply(9, listOf(listOf("abc", "def"), listOf("other"))), 9))
        assertEquals(DnsMessage.Reply(0, false, listOf("abcdef", "other")), parsed)
        assertEquals(DnsMessage.Reply(3, false, emptyList<String>()), DnsMessage.parse(reply(9, emptyList(), rcode = 3), 9))
        assertNull("another query's reply", DnsMessage.parse(reply(9, listOf(listOf("x"))), 10))
        assertNull("a query is no reply", DnsMessage.parse(DnsMessage.query(9, name), 9))
        assertNull("too short", DnsMessage.parse(ByteArray(5), 0))
        assertNull("a cut reply without TC is malformed", DnsMessage.parse(reply(9, listOf(listOf("abc")), cut = 2), 9))
    }

    @Test fun truncatedRepliesKeepTheRecordsThatArrivedComplete() {
        val parsed = checkNotNull(DnsMessage.parse(reply(5, listOf(listOf("first"), listOf("second")), truncated = true, cut = 3), 5))
        assertTrue(parsed.truncated)
        assertEquals(listOf("first"), parsed.txt)
    }

    @Test fun udpLookupMatchesRepliesToServersAndReportsSilenceAndMissingAddresses() {
        DatagramSocket(InetSocketAddress(InetAddress.getByName("127.0.0.1"), 0)).use { server ->
            val received = java.util.Collections.synchronizedList(mutableListOf<ByteArray>())
            // Both test servers share this socket; only the first query (ns1's, sent first) is answered.
            val responder = Thread {
                val buffer = ByteArray(512)
                try {
                    while (true) {
                        val packet = DatagramPacket(buffer, buffer.size)
                        server.receive(packet)
                        received.add(buffer.copyOf(packet.length))
                        if (received.size > 1) continue
                        val id = (buffer[0].toInt() and 255) shl 8 or (buffer[1].toInt() and 255)
                        val answer = reply(id, listOf(listOf("value")))
                        server.send(DatagramPacket(answer, answer.size, packet.socketAddress))
                    }
                } catch (_: Exception) { }
            }.apply { isDaemon = true; start() }
            val lookup = UdpTxtLookup(port = server.localPort, timeoutMs = 1_000) { host ->
                when (host) {
                    "ns1.test", "ns2.test" -> listOf(InetAddress.getByName("127.0.0.1"))
                    else -> throw UnknownHostException(host)
                }
            }
            assertEquals(listOf(TxtResult.Answer(listOf("value")), TxtResult.NoAnswer, TxtResult.Unreachable),
                lookup.query(listOf("ns1.test", "ns2.test", "ns3.test"), name))
            assertEquals(2, received.size)
            received.forEach { query -> assertArrayEquals(DnsMessage.query(((query[0].toInt() and 255) shl 8) or (query[1].toInt() and 255), name), query) }
            server.close()
            responder.join(1_000)
        }
    }

    private class Clock(var now: Long = 0)
    private fun propagation(clock: Clock, results: (Int) -> List<TxtResult>): Pair<TxtPropagation, MutableList<Long>> {
        val pauses = mutableListOf<Long>()
        var round = 0
        val lookup = TxtLookup { _, _ -> results(round++).also { clock.now += 100 } }
        return TxtPropagation(lookup, Pause { pauses.add(it); clock.now += it }, { clock.now }) to pauses
    }

    @Test fun propagationWaitsUntilEveryAnsweringServerShowsTheValue() {
        val clock = Clock()
        val (check, pauses) = propagation(clock) { round ->
            if (round < 2) listOf(TxtResult.Answer(listOf("value")), TxtResult.Answer(listOf("old")), TxtResult.NoAnswer)
            else listOf(TxtResult.Answer(listOf("value", "old")), TxtResult.Answer(listOf("value")), TxtResult.NoAnswer)
        }
        check.await(listOf("a", "b", "c"), name, "value")
        assertEquals(listOf(5_000L, 5_000L, 5_000L), pauses) // Two polls, then the settling wait.
    }

    @Test fun propagationGivesUpAfterTwoMinutesWhenAnAnsweringServerNeverShowsTheValue() {
        val clock = Clock()
        val (check, pauses) = propagation(clock) { listOf(TxtResult.Answer(listOf("old"))) }
        val failure = assertThrows(PublicNameFailure::class.java) { check.await(listOf("a"), name, "value") }
        assertEquals(PublicNameError.DNS_NOT_UPDATED, failure.error)
        assertTrue(clock.now <= TxtPropagation.LIMIT_MS)
        assertTrue(pauses.size >= 20)
    }

    @Test fun propagationWaitsAFixedMinuteWhenNoServerCanBeQueried() {
        val clock = Clock()
        val (unreachable, pauses) = propagation(clock) { listOf(TxtResult.Unreachable, TxtResult.Unreachable) }
        unreachable.await(listOf("a", "b"), name, "value")
        assertEquals(listOf(60_000L), pauses)
        // Servers that never answer (outgoing DNS blocked) count the same after a minute of silence.
        val silent = Clock()
        val (quiet, quietPauses) = propagation(silent) { listOf(TxtResult.NoAnswer, TxtResult.Unreachable) }
        quiet.await(listOf("a", "b"), name, "value")
        assertTrue(silent.now in 60_000L..66_000L)
        assertTrue(quietPauses.all { it == 5_000L })
        // A truncated reply without the value is no answer either way.
        val cut = Clock()
        val (truncated, _) = propagation(cut) { listOf(TxtResult.Answer(emptyList(), truncated = true)) }
        truncated.await(listOf("a"), name, "value")
        assertTrue(cut.now in 60_000L..66_000L)
    }
}
