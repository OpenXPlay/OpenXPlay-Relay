// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-15
package org.openxplay.direlay.admin

import java.security.MessageDigest
import java.security.SecureRandom
import java.util.Base64
import javax.crypto.SecretKeyFactory
import javax.crypto.spec.PBEKeySpec

interface AdminAuthStore {
    fun read(): AdminAuthState
    /** Replace the entire state durably before returning. */
    fun write(state: AdminAuthState)
}
data class AdminPasswordHash(val salt: String, val hash: String)
data class AdminAuthState(val password: AdminPasswordHash? = null, val sessions: Map<String, Long> = emptyMap())
fun interface AdminClock { fun millis(): Long }

class AdminAuth(private val store: AdminAuthStore, private val clock: AdminClock = AdminClock(System::currentTimeMillis),
    private val random: SecureRandom = SecureRandom()) {
    enum class Failure { PASSWORD, RATE_LIMIT, FORMAT }
    data class Login(val token: String? = null, val failure: Failure? = null)
    private val failures = mutableMapOf<String, MutableList<Long>>()
    val defaultPassword: Boolean @Synchronized get() = store.read().password == null

    @Synchronized fun login(address: String, password: String): Login {
        val failure = verify(address, password)
        if (failure != null) return Login(failure = failure)
        val token = encode(ByteArray(32).also(random::nextBytes))
        val state = store.read()
        store.write(state.copy(sessions = liveSessions(state) + (digest(token) to clock.millis() + SESSION_MILLIS)))
        return Login(token)
    }
    @Synchronized fun validSession(token: String?): Boolean {
        if (token == null || !token.matches(Regex("[A-Za-z0-9_-]{43}"))) return false
        return (store.read().sessions[digest(token)] ?: return false) > clock.millis()
    }
    @Synchronized fun logout(token: String?) {
        if (token != null) store.read().let { store.write(it.copy(sessions = liveSessions(it) - digest(token))) }
    }
    @Synchronized fun changePassword(address: String, current: String, next: String): Failure? {
        verify(address, current)?.let { return it }
        if (next.length < 8) return Failure.FORMAT
        val salt = ByteArray(16).also(random::nextBytes)
        // A password change invalidates every previously issued session.
        store.write(AdminAuthState(AdminPasswordHash(encode(salt), encode(derive(next, salt)))))
        return null
    }
    @Synchronized fun reset() { store.write(AdminAuthState()); failures.clear() }
    private fun liveSessions(state: AdminAuthState) = state.sessions.filterValues { it > clock.millis() }
    private fun verify(address: String, password: String): Failure? {
        val now = clock.millis()
        failures.entries.removeAll { (_, times) -> times.removeAll { it <= now - 60_000 }; times.isEmpty() }
        val attempts = failures.getOrPut(address) { mutableListOf() }
        if (attempts.size >= 5) return Failure.RATE_LIMIT
        val saved = store.read().password
        val valid = if (saved == null) MessageDigest.isEqual(password.toByteArray(), DEFAULT_PASSWORD.toByteArray())
            else MessageDigest.isEqual(derive(password, Base64.getUrlDecoder().decode(saved.salt)), Base64.getUrlDecoder().decode(saved.hash))
        if (valid) { failures.remove(address); return null }
        attempts.add(now)
        return if (attempts.size >= 5) Failure.RATE_LIMIT else Failure.PASSWORD
    }
    private fun derive(password: String, salt: ByteArray): ByteArray {
        val spec = PBEKeySpec(password.toCharArray(), salt, 120_000, 256)
        return try { SecretKeyFactory.getInstance("PBKDF2WithHmacSHA256").generateSecret(spec).encoded }
        finally { spec.clearPassword() }
    }
    private fun digest(token: String) = encode(MessageDigest.getInstance("SHA-256").digest(token.toByteArray(Charsets.US_ASCII)))
    private fun encode(bytes: ByteArray) = Base64.getUrlEncoder().withoutPadding().encodeToString(bytes)
    companion object {
        const val DEFAULT_PASSWORD = "openxplay"
        const val SESSION_MILLIS = 30L * 24 * 60 * 60 * 1000
    }
}
