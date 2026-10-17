// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-15
package org.openxplay.direlay.admin

import org.json.JSONException
import org.json.JSONObject
import java.net.InetAddress
import java.net.URI
import java.net.URLDecoder

fun interface AdminAssets { fun read(path: String): ByteArray? }
data class AdminRequest(val method: String, val target: String, val headers: Map<String, String> = emptyMap(),
    val body: ByteArray = byteArrayOf()) {
    fun header(name: String): String? = headers.entries.firstOrNull { it.key.equals(name, true) }?.value
}
data class AdminResponse(val status: Int, val body: ByteArray, val headers: Map<String, String> = emptyMap())

fun interface AdminHandler {
    fun route(peer: InetAddress, request: AdminRequest): AdminResponse
    fun responseSent() {}
    fun responseFailed() {}
}

/** Transport-independent contract boundary; tests exercise precisely the production routes. */
class AdminRouter(private val admin: RelayAdmin, private val auth: AdminAuth, private val assets: AdminAssets) : AdminHandler {
    override fun responseSent() = admin.responseSent()
    override fun responseFailed() = admin.responseFailed()
    override fun route(peer: InetAddress, request: AdminRequest): AdminResponse {
        if (!AdminAccess.isPrivate(peer)) return error(403, "error.request")
        if (request.body.size > 65536) return error(413, "error.request")
        val path = try {
            require(request.target.startsWith("/") && !request.target.startsWith("//"))
            val raw = URI(request.target).rawPath
            val decoded = URLDecoder.decode(raw.replace("+", "%2B"), "UTF-8")
            require(decoded.none { it == '\\' || it.code < 32 || it == '\u007f' } &&
                decoded.split('/').none { it == "." || it == ".." } && '%' !in decoded)
            decoded
        } catch (_: Exception) { return error(400, "error.request") }
        if (!path.startsWith("/api/")) return static(request.method, path)
        val method = request.method
        val token = request.header("Cookie")?.split(';')?.map { it.trim() }
            ?.firstOrNull { it.startsWith("oxp_admin=") }?.substringAfter('=')
        if (path != "/api/v0/session" && path != "/api/v0/login" && !auth.validSession(token))
            return error(401, "error.request")
        if (method !in listOf("GET", "HEAD")) {
            if (request.header("X-OpenXPlay-Admin") != "1") return error(403, "error.request")
            if (request.header("Content-Type")?.substringBefore(';')?.trim()?.lowercase() != "application/json")
                return error(415, "error.request")
        }
        return try {
            fun body() = if (request.body.isEmpty()) JSONObject() else JSONObject(String(request.body, Charsets.UTF_8))
            when (method to path) {
                "GET" to "/api/v0/session" -> json(200, obj("state" to if (auth.validSession(token)) "ready" else "login", "defaultPassword" to auth.defaultPassword))
                "POST" to "/api/v0/login" -> {
                    val result = auth.login(peer.hostAddress.orEmpty(), body().getString("password"))
                    if (result.failure != null) authError(result.failure)
                    else json(200, obj("ok" to true), mapOf("Set-Cookie" to cookie(result.token!!)))
                }
                "POST" to "/api/v0/password" -> {
                    val data = body()
                    val failure = auth.changePassword(peer.hostAddress.orEmpty(), data.getString("current"), data.getString("new"))
                    if (failure != null) authError(failure)
                    else json(200, obj("ok" to true), mapOf("Set-Cookie" to cookie("", 0)))
                }
                "POST" to "/api/v0/logout" -> {
                    auth.logout(token); json(200, obj("ok" to true), mapOf("Set-Cookie" to cookie("", 0)))
                }
                "GET" to "/api/v0/status" -> {
                    // Diagnostics are large and change every second; only the developer section asks.
                    val diagnostics = URI(request.target).rawQuery?.split('&')?.contains("diagnostics=1") == true
                    json(200, admin.status().let { if (diagnostics) it.copy(receivers = admin.receivers()) else it }.json())
                }
                "GET" to "/api/v0/settings" -> json(200, admin.settings().json())
                "PUT" to "/api/v0/settings" -> {
                    val data = body()
                    val result = admin.update(data.keys().asSequence().associateWith { readJsonValue(data.get(it)) })
                    json(if (result.errors.isEmpty()) 200 else 422, result.json())
                }
                "GET" to "/api/v0/phones" -> json(200, jsonValue(admin.phones()))
                else -> if (method == "POST" && path.startsWith("/api/v0/actions/")) {
                    val result = admin.action(path.removePrefix("/api/v0/actions/"))
                    json(if (result.error == null) 200 else 409, result.json())
                } else error(404, "error.request")
            }
        } catch (_: JSONException) { error(400, "error.invalid") }
          catch (_: IllegalArgumentException) { error(400, "error.invalid") }
    }
    private fun static(method: String, path: String): AdminResponse {
        if (method !in listOf("GET", "HEAD")) return error(405, "error.request")
        val file = if (path == "/") "index.html" else path.removePrefix("/")
        val bytes = assets.read(file) ?: return error(404, "error.request")
        val mime = when (file.substringAfterLast('.').lowercase()) {
            "html" -> "text/html; charset=utf-8"; "js", "mjs" -> "text/javascript; charset=utf-8"
            "css" -> "text/css; charset=utf-8"; "json", "map" -> "application/json"
            "svg" -> "image/svg+xml"; "png" -> "image/png"; "ico" -> "image/x-icon"
            "jpg", "jpeg" -> "image/jpeg"; "webp" -> "image/webp"; "woff2" -> "font/woff2"
            "woff" -> "font/woff"; "txt" -> "text/plain; charset=utf-8"; else -> "application/octet-stream"
        }
        return AdminResponse(200, if (method == "HEAD") byteArrayOf() else bytes,
            mapOf("Content-Type" to mime, "Cache-Control" to if (file == "index.html") "no-cache" else "public, max-age=3600"))
    }
    private fun authError(failure: AdminAuth.Failure) = when (failure) {
        AdminAuth.Failure.RATE_LIMIT -> json(429, obj("error" to "error.rateLimit"), mapOf("Retry-After" to "60"))
        AdminAuth.Failure.FORMAT -> error(422, "error.passwordFormat")
        AdminAuth.Failure.PASSWORD -> error(401, "password")
    }
    private fun cookie(token: String, age: Int = 2592000) = "oxp_admin=$token; HttpOnly; Secure; SameSite=Strict; Path=/; Max-Age=$age"
    private fun error(status: Int, key: String) = json(status, obj("error" to key))
    private fun json(status: Int, value: Any, headers: Map<String, String> = emptyMap()) =
        AdminResponse(status, value.toString().toByteArray(Charsets.UTF_8), mapOf("Content-Type" to "application/json; charset=utf-8", "Cache-Control" to "no-store") + headers)
}
