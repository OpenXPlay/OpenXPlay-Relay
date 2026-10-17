// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-15
package org.openxplay.direlay.admin

import java.io.BufferedInputStream
import java.io.ByteArrayOutputStream
import java.io.InputStream
import java.net.InetSocketAddress
import java.net.Socket
import java.util.concurrent.ArrayBlockingQueue
import java.util.concurrent.ConcurrentHashMap
import java.util.concurrent.ThreadPoolExecutor
import java.util.concurrent.TimeUnit
import javax.net.ssl.SSLContext
import javax.net.ssl.SSLServerSocket

/** One bounded request per TLS connection; workers and queued sockets are both bounded. */
class AdminServer(private val router: AdminHandler) : AutoCloseable {
    private var listener: SSLServerSocket? = null
    private var acceptor: Thread? = null
    private var workers: ThreadPoolExecutor? = null
    private val sockets = ConcurrentHashMap.newKeySet<Socket>()
    @Synchronized fun start(context: SSLContext, port: Int = PORT) {
        if (listener != null) return
        val socket = context.serverSocketFactory.createServerSocket() as SSLServerSocket
        try {
            socket.reuseAddress = true
            socket.bind(InetSocketAddress("0.0.0.0", port), 16)
            socket.enabledProtocols = socket.supportedProtocols.filter { it == "TLSv1.2" || it == "TLSv1.3" }.toTypedArray()
        } catch (failure: Exception) { socket.close(); throw failure }
        val pool = ThreadPoolExecutor(4, 4, 0, TimeUnit.SECONDS, ArrayBlockingQueue(8),
            { task -> Thread(task, "direlay-admin-client").apply { isDaemon = true } })
        workers = pool; listener = socket
        acceptor = Thread({
            while (!socket.isClosed) {
                val client = try { socket.accept() } catch (_: Exception) { break }
                sockets.add(client)
                try { pool.execute { serve(client) } }
                catch (_: java.util.concurrent.RejectedExecutionException) { sockets.remove(client); client.close() }
            }
        }, "direlay-admin-listen").apply { isDaemon = true; start() }
    }
    private fun serve(client: Socket) {
        try { client.use {
            it.soTimeout = 10_000
            val response = if (!AdminAccess.isPrivate(it.inetAddress)) failure(403) else try {
                router.route(it.inetAddress, readRequest(BufferedInputStream(it.getInputStream())))
            } catch (problem: HttpFailure) { failure(problem.status) }
              catch (_: Exception) { failure(400) }
            val output = it.getOutputStream()
            val headers = mapOf("Content-Length" to response.body.size.toString(), "Connection" to "close",
                "X-Content-Type-Options" to "nosniff", "Referrer-Policy" to "no-referrer") + response.headers
            output.write(("HTTP/1.1 ${response.status} ${reason(response.status)}\r\n" +
                headers.entries.joinToString("") { (key, value) -> "$key: $value\r\n" } + "\r\n").toByteArray(Charsets.US_ASCII))
            output.write(response.body); output.flush()
            router.responseSent()
        } } catch (_: Exception) { /* A timeout or disconnect releases this worker. */ }
        finally { router.responseFailed(); sockets.remove(client); runCatching { client.close() } }
    }
    @Synchronized override fun close() {
        listener?.close(); listener = null
        sockets.forEach { runCatching { it.close() } }; sockets.clear()
        workers?.shutdownNow(); workers = null; acceptor = null
    }
    private fun failure(status: Int) = AdminResponse(status, "{\"error\":\"error.request\"}".toByteArray(), mapOf("Content-Type" to "application/json"))
    private fun reason(status: Int) = when (status) {
        200 -> "OK"; 400 -> "Bad Request"; 401 -> "Unauthorized"; 403 -> "Forbidden"; 404 -> "Not Found"
        405 -> "Method Not Allowed"; 409 -> "Conflict"; 413 -> "Content Too Large"; 415 -> "Unsupported Media Type"
        422 -> "Unprocessable Content"; 429 -> "Too Many Requests"; 431 -> "Request Header Fields Too Large"; else -> "Error"
    }
    internal class HttpFailure(val status: Int) : Exception()
    companion object {
        const val PORT = 6975
        internal fun readRequest(input: InputStream): AdminRequest {
            val bytes = ByteArrayOutputStream()
            var matched = 0
            val end = byteArrayOf(13, 10, 13, 10)
            while (matched < 4) {
                val b = input.read()
                if (b < 0) throw HttpFailure(400)
                bytes.write(b)
                if (bytes.size() > 16384) throw HttpFailure(431)
                matched = if (b == end[matched].toInt()) matched + 1 else if (b == 13) 1 else 0
            }
            val lines = bytes.toString("US-ASCII").removeSuffix("\r\n\r\n").split("\r\n")
            val request = lines.first().split(' ')
            if (request.size != 3 || request[2] != "HTTP/1.1" || !request[0].matches(Regex("[A-Z]+"))) throw HttpFailure(400)
            val headers = linkedMapOf<String, String>()
            for (line in lines.drop(1)) {
                val key = line.substringBefore(':').lowercase()
                if (':' !in line || !key.matches(Regex("[a-z0-9!#$%&'*+.^_`|~-]+")) || key in headers) throw HttpFailure(400)
                val value = line.substringAfter(':').trim()
                if (value.any { it.code < 32 && it != '\t' }) throw HttpFailure(400)
                headers[key] = value
            }
            if (headers["host"].isNullOrBlank() || "transfer-encoding" in headers) throw HttpFailure(400)
            val length = headers["content-length"]?.let {
                if (!it.matches(Regex("[0-9]+"))) throw HttpFailure(400)
                it.toLongOrNull() ?: throw HttpFailure(413)
            } ?: 0
            if (length > 65536) throw HttpFailure(413)
            val body = ByteArray(length.toInt())
            var read = 0
            while (read < body.size) { val n = input.read(body, read, body.size - read); if (n <= 0) throw HttpFailure(400); read += n }
            return AdminRequest(request[0], request[1], headers, body)
        }
    }
}
