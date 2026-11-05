// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-16
package org.openxplay.direlay.clink

/** Successful request types survive restarts; subscription persistence is explicitly selected. */
internal class BlePeerPolicy(private val store: Store = object : Store {
    override fun read(key: String): Int? = null
    override fun write(key: String, type: Int) {}
}, private val subscriptions: SubscriptionStore? = null,
    /**
     * Persistence trace for post-hoc analysis of each connection. Injected instead of calling
     * android.util.Log so the JVM unit tests stay pure and can assert on the same lines.
     */
    private val log: (String) -> Unit = {}) {
    interface Store {
        fun read(key: String): Int?
        fun write(key: String, type: Int)
    }
    interface SubscriptionStore {
        fun readLegacy(key: String): Boolean
        fun writeLegacy(key: String)
        fun removeLegacy(key: String)
    }
    data class Choice(val requestType: Int = 1002, val legacy: Boolean = false)
    private val peers = mutableMapOf<String, Choice>()
    private val persistedLegacy = mutableSetOf<String>()
    fun key(carId: String, address: String): String =
        java.security.MessageDigest.getInstance("SHA-256")
            .digest((if (carId.isNotBlank()) "car:$carId" else "address:$address").toByteArray(Charsets.UTF_8))
            .joinToString("") { "%02x".format(it.toInt() and 255) }
    /** Short key keeps one connection's lines greppable; the full hash is the storage key. */
    private fun trace(key: String, message: String) = log("peer_$message key=${key.take(8)}")
    @Synchronized fun choice(key: String): Choice {
        peers[key]?.let { return it }
        val type = store.read(key)?.takeIf { it == 1001 || it == 1002 } ?: 1002
        val legacy = subscriptions?.readLegacy(key) == true
        if (legacy) persistedLegacy.add(key)
        trace(key, "choice legacy=$legacy type=$type source=${if (legacy) "persisted" else "default"}")
        return Choice(type, legacy)
    }
    @Synchronized fun cccdTimedOut(key: String, choice: Choice) {
        peers[key] = choice.copy(legacy = true)
        trace(key, "cccd_timeout legacy=true type=${choice.requestType} where=memory")
    }
    @Synchronized fun rejected(key: String, choice: Choice) {
        peers[key] = choice.copy(requestType = if (choice.requestType == 1002) 1001 else 1002)
        trace(key, "rejected type=${peers[key]?.requestType} legacy=${choice.legacy} where=memory")
    }
    @Synchronized fun succeeded(key: String, choice: Choice) {
        peers[key] = choice
        store.write(key, choice.requestType)
        trace(key, "succeeded type=${choice.requestType} legacy=${choice.legacy} persisted=type")
    }
    @Synchronized fun exchanged(key: String, choice: Choice) {
        if (choice.legacy) {
            subscriptions?.writeLegacy(key)
            trace(key, "exchanged legacy=true persisted=legacy")
        } else {
            trace(key, "exchanged legacy=false persisted=none")
        }
    }
    @Synchronized fun exchangeFailed(key: String) {
        if (persistedLegacy.remove(key)) {
            subscriptions?.removeLegacy(key)
            peers[key]?.let { peers[key] = it.copy(legacy = false) }
            trace(key, "exchange_failed legacy=false forgot=persisted")
        } else {
            trace(key, "exchange_failed legacy=false forgot=none")
        }
    }
}
