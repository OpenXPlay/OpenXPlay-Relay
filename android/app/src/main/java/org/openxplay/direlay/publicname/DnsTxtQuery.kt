// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-20
package org.openxplay.direlay.publicname

import java.io.ByteArrayOutputStream
import java.io.IOException
import java.net.DatagramPacket
import java.net.DatagramSocket
import java.net.Inet4Address
import java.net.InetAddress
import java.net.InetSocketAddress
import java.net.SocketTimeoutException
import java.security.SecureRandom

/** One name server's reply to a TXT query. */
sealed class TxtResult {
    /** NOERROR or NXDOMAIN with the TXT values it carried; each value joins the strings of one record. */
    data class Answer(val values: List<String>, val truncated: Boolean = false) : TxtResult()
    /** Queried, but no usable reply in time. */
    data object NoAnswer : TxtResult()
    /** Could not be queried at all (no address, or the query could not be sent). */
    data object Unreachable : TxtResult()
}

fun interface TxtLookup { fun query(servers: List<String>, name: String): List<TxtResult> }

/** The two DNS messages the propagation check needs: a TXT query and the TXT strings of its reply. */
object DnsMessage {
    const val TYPE_TXT = 16
    private const val CLASS_IN = 1

    /** A standard query without recursion: the provider's authoritative servers are asked directly. */
    fun query(id: Int, name: String): ByteArray {
        val out = ByteArrayOutputStream()
        fun u16(value: Int) { out.write(value shr 8 and 255); out.write(value and 255) }
        u16(id); u16(0); u16(1); u16(0); u16(0); u16(0)
        name.trimEnd('.').split('.').forEach { label ->
            val bytes = label.toByteArray(Charsets.US_ASCII)
            require(bytes.size in 1..63) { "Invalid DNS label" }
            out.write(bytes.size); out.write(bytes)
        }
        out.write(0); u16(TYPE_TXT); u16(CLASS_IN)
        require(out.size() <= 512) { "Name too long" }
        return out.toByteArray()
    }

    data class Reply(val rcode: Int, val truncated: Boolean, val txt: List<String>)

    /**
     * Null when the message is not a reply to query [id]. A truncated reply keeps the records that
     * arrived complete.
     */
    fun parse(message: ByteArray, id: Int): Reply? {
        if (message.size < 12) return null
        fun u8(at: Int): Int { if (at >= message.size) throw IndexOutOfBoundsException(); return message[at].toInt() and 255 }
        fun u16(at: Int) = u8(at) shl 8 or u8(at + 1)
        if (u16(0) != id) return null
        val flags = u16(2)
        if (flags and 0x8000 == 0 || flags shr 11 and 15 != 0) return null
        val truncated = flags and 0x0200 != 0
        val questions = u16(4)
        val answers = u16(6)
        val values = mutableListOf<String>()
        try {
            var at = 12
            repeat(questions) { at = skipName(message, at) + 4 }
            repeat(answers) {
                at = skipName(message, at)
                val type = u16(at); val klass = u16(at + 2); val length = u16(at + 8)
                val data = at + 10
                if (data + length > message.size) throw IndexOutOfBoundsException()
                if (type == TYPE_TXT && klass == CLASS_IN) {
                    val text = StringBuilder()
                    var position = data
                    while (position < data + length) {
                        val size = u8(position)
                        if (position + 1 + size > data + length) throw IndexOutOfBoundsException()
                        text.append(String(message, position + 1, size, Charsets.ISO_8859_1))
                        position += 1 + size
                    }
                    values.add(text.toString())
                }
                at = data + length
            }
        } catch (_: IndexOutOfBoundsException) {
            if (!truncated) return null
        }
        return Reply(flags and 15, truncated, values)
    }

    private fun skipName(message: ByteArray, start: Int): Int {
        var at = start
        repeat(128) {
            if (at >= message.size) throw IndexOutOfBoundsException()
            val length = message[at].toInt() and 255
            when {
                length == 0 -> return at + 1
                length and 0xC0 == 0xC0 -> { if (at + 1 >= message.size) throw IndexOutOfBoundsException(); return at + 2 }
                length and 0xC0 != 0 -> throw IndexOutOfBoundsException()
                else -> at += length + 1
            }
        }
        throw IndexOutOfBoundsException()
    }
}

/** Sends one query to every server from a single socket and waits up to [timeoutMs] for the replies. */
class UdpTxtLookup(private val port: Int = 53, private val timeoutMs: Int = 3_000,
    private val resolve: (String) -> List<InetAddress> = { InetAddress.getAllByName(it).toList() }) : TxtLookup {
    private val random = SecureRandom()

    override fun query(servers: List<String>, name: String): List<TxtResult> {
        val targets = servers.map { server ->
            runCatching { resolve(server).sortedBy { if (it is Inet4Address) 0 else 1 }.firstOrNull() }.getOrNull()
        }
        val results = MutableList<TxtResult>(servers.size) { if (targets[it] == null) TxtResult.Unreachable else TxtResult.NoAnswer }
        if (targets.all { it == null }) return results
        val ids = IntArray(servers.size) { random.nextInt(65_536) }
        val waiting = BooleanArray(servers.size)
        try {
            DatagramSocket().use { socket ->
                targets.forEachIndexed { index, address ->
                    if (address == null) return@forEachIndexed
                    val message = DnsMessage.query(ids[index], name)
                    try { socket.send(DatagramPacket(message, message.size, InetSocketAddress(address, port))); waiting[index] = true }
                    catch (_: IOException) { results[index] = TxtResult.Unreachable }
                }
                val deadline = System.nanoTime() + timeoutMs * 1_000_000L
                val buffer = ByteArray(4096)
                while (waiting.any { it }) {
                    val left = (deadline - System.nanoTime()) / 1_000_000L
                    if (left <= 0) break
                    socket.soTimeout = left.toInt().coerceAtLeast(1)
                    val packet = DatagramPacket(buffer, buffer.size)
                    try { socket.receive(packet) } catch (_: SocketTimeoutException) { break }
                    if (packet.length < 2 || packet.port != port) continue
                    val id = (buffer[0].toInt() and 255) shl 8 or (buffer[1].toInt() and 255)
                    val index = targets.indices.firstOrNull { waiting[it] && ids[it] == id && targets[it] == packet.address } ?: continue
                    val reply = DnsMessage.parse(buffer.copyOf(packet.length), id) ?: continue
                    waiting[index] = false
                    results[index] = if (reply.rcode == 0 || reply.rcode == 3) TxtResult.Answer(reply.txt, reply.truncated) else TxtResult.NoAnswer
                }
            }
        } catch (_: IOException) { /* No socket: the queried servers count as unanswered. */ }
        return results
    }
}

/**
 * Polls the provider's name servers every 5 s for up to 120 s, until every server that answers shows
 * the value (at least one must); then waits 5 s more. When no server can be queried at all (no address
 * for any of them, or none has answered for 60 s, as on networks that block outgoing DNS) it waits a
 * fixed 60 s instead and lets the CA check.
 */
class TxtPropagation(private val lookup: TxtLookup, private val pause: Pause, private val clock: () -> Long) {
    fun await(servers: List<String>, name: String, value: String) {
        val start = clock()
        var answered = false
        while (true) {
            val results = lookup.query(servers, name)
            if (!answered && results.all { it == TxtResult.Unreachable }) { pause.pause(FIXED_WAIT_MS); return }
            // A truncated reply counts only when the value arrived anyway.
            val answers = results.filterIsInstance<TxtResult.Answer>().filter { !it.truncated || value in it.values }
            if (answers.isNotEmpty()) answered = true
            if (answers.isNotEmpty() && answers.all { value in it.values }) { pause.pause(SETTLE_MS); return }
            val elapsed = clock() - start
            if (!answered && elapsed >= FIXED_WAIT_MS) return
            if (elapsed + INTERVAL_MS > LIMIT_MS) throw PublicNameFailure(PublicNameError.DNS_NOT_UPDATED)
            pause.pause(INTERVAL_MS)
        }
    }

    companion object {
        const val INTERVAL_MS = 5_000L
        const val LIMIT_MS = 120_000L
        const val SETTLE_MS = 5_000L
        const val FIXED_WAIT_MS = 60_000L
    }
}
