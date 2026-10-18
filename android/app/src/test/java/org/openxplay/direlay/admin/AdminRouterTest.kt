// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-15
package org.openxplay.direlay.admin

import android.app.Application
import java.net.InetAddress
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [28], manifest = Config.NONE, application = Application::class)
class AdminRouterTest {
    private val privatePeer = InetAddress.getByName("192.168.1.2")
    private val auth = AdminAuth(MemoryAdminAuthStore())
    private var updated: Map<String, JsonElement>? = null
    private var action: String? = null
    private val fake = object : RelayAdmin {
        override fun status() = AdminStatus("android", "OpenXPlay Relay", "test", "openxplay-android.local", "https://openxplay-android.local:6975/",
            emptyList(), "idle", false, false, AdminIphone(null, "none"), AdminNetwork("wifi-direct", null), "web", null, emptyList())
        override fun settings() = AdminSettings(listOf(AdminSection("output", listOf(AdminField("autoStart", "bool")))), mapOf("autoStart" to false))
        override fun update(values: Map<String, JsonElement>): AdminUpdate {
            updated = values
            return if (values["autoStart"] is Boolean) AdminUpdate("next-start") else AdminUpdate(errors = mapOf("autoStart" to "error.invalid"))
        }
        override fun action(name: String): AdminResult { action = name; return AdminResult(if (name == "start") null else "error.conflict") }
        override fun phones() = listOf(AdminPhone("id", "iPhone", "phone", true, true, false))
        override fun receivers() = listOf(AdminReceiver("1", 0.2, """{"type":"receiver-info","refreshHz":60}""",
            listOf("""{"type":"stats","renderedFps":59.9}""")))
    }
    private val router = AdminRouter(fake, auth, AdminAssets { when (it) { "index.html" -> "<html>admin</html>".toByteArray(); "assets/main.js" -> "script".toByteArray(); else -> null } })
    private lateinit var cookie: String
    @Before fun login() { cookie = "oxp_admin=" + auth.login("test", "openxplay").token!! }
    private fun call(path: String, method: String = "GET", body: String = "", session: Boolean = true,
        csrf: Boolean = true, peer: InetAddress = privatePeer, contentType: String = "application/json"): AdminResponse {
        val headers = mutableMapOf("Content-Type" to contentType)
        if (session) headers["Cookie"] = cookie
        if (csrf) headers["X-OpenXPlay-Admin"] = "1"
        return router.route(peer, AdminRequest(method, path, headers, body.toByteArray()))
    }
    private fun data(response: AdminResponse) = JSONObject(String(response.body))
    @Test fun sessionLoginAndSecureCookie() {
        assertEquals("login", data(call("/api/v0/session", session = false)).getString("state"))
        assertEquals("ready", data(call("/api/v0/session")).getString("state"))
        val response = call("/api/v0/login", "POST", "{\"password\":\"openxplay\"}", session = false)
        assertEquals(200, response.status)
        assertTrue(response.headers.getValue("Set-Cookie").endsWith("; HttpOnly; Secure; SameSite=Strict; Path=/; Max-Age=2592000"))
    }
    @Test fun statusSettingsPhonesAndUpdates() {
        assertEquals("android", data(call("/api/v0/status")).getString("platform"))
        assertTrue(data(call("/api/v0/settings")).has("schema"))
        assertTrue(String(call("/api/v0/phones").body).contains("iPhone"))
        val saved = call("/api/v0/settings", "PUT", "{\"autoStart\":true}")
        assertEquals(200, saved.status)
        assertEquals("next-start", data(saved).getString("applied"))
        assertEquals(true, updated!!["autoStart"])
        val invalid = call("/api/v0/settings", "PUT", "{\"autoStart\":3}")
        assertEquals(422, invalid.status)
        assertEquals("error.invalid", data(invalid).getJSONObject("errors").getString("autoStart"))
    }
    @Test fun actionsAndAuthenticationGuards() {
        assertEquals(200, call("/api/v0/actions/start", "POST", "{}").status)
        assertEquals("start", action)
        assertEquals(409, call("/api/v0/actions/unknown", "POST", "{}").status)
        assertEquals(401, call("/api/v0/status", session = false).status)
        assertEquals(403, call("/api/v0/actions/start", "POST", "{}", csrf = false).status)
        assertEquals(403, call("/api/v0/login", "POST", "{}", session = false, csrf = false).status)
        assertEquals(415, call("/api/v0/settings", "PUT", "{}", contentType = "text/plain").status)
        assertEquals(403, call("/", peer = InetAddress.getByName("8.8.8.8")).status)
    }
    @Test fun wrongPasswordAndRateLimit() {
        repeat(4) { assertEquals(401, call("/api/v0/login", "POST", "{\"password\":\"wrong\"}", session = false).status) }
        val limited = call("/api/v0/login", "POST", "{\"password\":\"wrong\"}", session = false)
        assertEquals(429, limited.status)
        assertEquals("60", limited.headers["Retry-After"])
    }
    @Test fun passwordChangeAndLogoutRevokeCookies() {
        assertEquals(422, call("/api/v0/password", "POST", "{\"current\":\"openxplay\",\"new\":\"short\"}").status)
        assertEquals(200, call("/api/v0/password", "POST", "{\"current\":\"openxplay\",\"new\":\"different password\"}").status)
        assertEquals(401, call("/api/v0/status").status)
        cookie = "oxp_admin=" + auth.login("test", "different password").token!!
        assertEquals(200, call("/api/v0/logout", "POST", "{}").status)
        assertEquals(401, call("/api/v0/status").status)
    }
    @Test fun staticFilesContentTypesMissingFilesAndTraversal() {
        val page = call("/", session = false)
        assertEquals(200, page.status)
        assertEquals("no-cache", page.headers["Cache-Control"])
        assertEquals("text/html; charset=utf-8", page.headers["Content-Type"])
        assertEquals("text/javascript; charset=utf-8", call("/assets/main.js", session = false).headers["Content-Type"])
        assertEquals(404, call("/missing").status)
        listOf("/../state", "/%2e%2e/state", "/assets/%2e%2e/index.html", "/%252e%252e/state", "/assets%5cstate", "//evil/x")
            .forEach { assertEquals(it, 400, call(it).status) }
        assertEquals(400, call("/api/v0/login", "POST", "invalid", session = false).status)
    }
    @Test fun nestedNetworkUpdatesAreDecodedWithoutLosingOmittedPasswords() {
        call("/api/v0/settings", "PUT", """{"sharedWifiNetworks":[{"ssid":"Home"},{"ssid":"Car","password":""}]}""")
        assertEquals(listOf(mapOf("ssid" to "Home"), mapOf("ssid" to "Car", "password" to "")), updated!!["sharedWifiNetworks"])
    }
    @Test fun statusIncludesReceiverDiagnosticsOnlyOnRequest() {
        assertFalse(data(call("/api/v0/status")).has("receivers"))
        assertFalse(data(call("/api/v0/status?diagnostics=0")).has("receivers"))
        val receiver = data(call("/api/v0/status?lang=en&diagnostics=1")).getJSONArray("receivers").getJSONObject(0)
        assertEquals("1", receiver.getString("client"))
        assertEquals(60, receiver.getJSONObject("info").getInt("refreshHz"))
        assertFalse(receiver.getJSONObject("info").has("type"))
        assertEquals(59.9, receiver.getJSONArray("stats").getJSONObject(0).getDouble("renderedFps"), 0.0)
    }
    @Test fun statusSerializesUnknownAndKnownHotspotsAndWifiControl() {
        val unknown = data(call("/api/v0/status"))
        assertTrue(unknown.has("hotspot"))
        assertTrue(unknown.isNull("hotspot"))
        assertFalse(unknown.getBoolean("wifiControl"))
        val known = fake.status().copy(hotspot = AdminHotspot("Phone", "passphrase", "192.168.43.1", true)).json()
        val hotspot = known.getJSONObject("hotspot")
        assertEquals("Phone", hotspot.getString("ssid"))
        assertEquals("passphrase", hotspot.getString("password"))
        assertEquals("192.168.43.1", hotspot.getString("ip"))
        assertTrue(hotspot.getBoolean("active"))
        val inactive = AdminHotspot("Phone", "passphrase", null, false).json()
        assertTrue(inactive.isNull("ip"))
        assertFalse(inactive.getBoolean("active"))
    }

}
