// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-20
package org.openxplay.direlay.publicname

import com.sun.net.httpserver.HttpServer
import java.net.InetAddress
import java.net.InetSocketAddress
import java.net.URLDecoder
import java.util.Collections

/** An in-process HTTP server on the loopback interface: the relay's real HttpURLConnection client talks to it. */
internal class FakeHttpServer(private val handler: (Request) -> Response) : AutoCloseable {
    class Request(val method: String, val path: String, val rawQuery: String?, val headers: Map<String, String>, val body: ByteArray) {
        val query: Map<String, String> get() = rawQuery.orEmpty().split('&').filter { it.isNotEmpty() }.associate {
            URLDecoder.decode(it.substringBefore('='), "UTF-8") to URLDecoder.decode(it.substringAfter('=', ""), "UTF-8")
        }
        val text: String get() = String(body, Charsets.UTF_8)
        fun header(name: String) = headers[name.lowercase()]
    }
    class Response(val status: Int, val body: ByteArray = ByteArray(0), val headers: Map<String, String> = emptyMap()) {
        constructor(status: Int, text: String, headers: Map<String, String> = emptyMap()) : this(status, text.toByteArray(), headers)
    }

    val requests: MutableList<Request> = Collections.synchronizedList(mutableListOf())
    /** Handler failures (test assertions included) answer 599; tests check this list. */
    val errors: MutableList<Throwable> = Collections.synchronizedList(mutableListOf())
    private val server = HttpServer.create(InetSocketAddress(InetAddress.getByName("127.0.0.1"), 0), 16).apply {
        createContext("/") { exchange ->
            val request = Request(exchange.requestMethod, exchange.requestURI.rawPath, exchange.requestURI.rawQuery,
                exchange.requestHeaders.entries.associate { it.key.lowercase() to it.value.joinToString(",") },
                exchange.requestBody.readBytes())
            requests.add(request)
            val response = try { handler(request) } catch (failure: Throwable) { errors.add(failure); Response(599, failure.toString()) }
            response.headers.forEach { (key, value) -> exchange.responseHeaders.add(key, value) }
            val empty = response.body.isEmpty() || request.method == "HEAD" || response.status == 204
            exchange.sendResponseHeaders(response.status, if (empty) -1 else response.body.size.toLong())
            if (!empty) exchange.responseBody.use { it.write(response.body) } else exchange.close()
        }
        start()
    }
    val base: String get() = "http://127.0.0.1:${server.address.port}"

    override fun close() { server.stop(0) }
}
