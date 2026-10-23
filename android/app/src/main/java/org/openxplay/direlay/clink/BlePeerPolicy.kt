// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-16
package org.openxplay.direlay.clink

/** Successful request types survive restarts; subscription persistence is explicitly selected. */
internal class BlePeerPolicy(private val store: Store = object : Store {
    override fun read(key: String): Int? = null
    override fun write(key: String, type: Int) {}
}, private val subscriptions: SubscriptionStore? = null) {
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
    @Synchronized fun choice(key: String): Choice {
        peers[key]?.let { return it }
        val type = store.read(key)?.takeIf { it == 1001 || it == 1002 } ?: 1002
        val legacy = subscriptions?.readLegacy(key) == true
        if (legacy) persistedLegacy.add(key)
        return Choice(type, legacy)
    }
    @Synchronized fun cccdTimedOut(key: String, choice: Choice) {
        peers[key] = choice.copy(legacy = true)
    }
    @Synchronized fun rejected(key: String, choice: Choice) {
        peers[key] = choice.copy(requestType = if (choice.requestType == 1002) 1001 else 1002)
    }
    @Synchronized fun succeeded(key: String, choice: Choice) {
        peers[key] = choice
        store.write(key, choice.requestType)
    }
    @Synchronized fun exchanged(key: String, choice: Choice) {
        if (choice.legacy) subscriptions?.writeLegacy(key)
    }
    @Synchronized fun exchangeFailed(key: String) {
        if (persistedLegacy.remove(key)) {
            subscriptions?.removeLegacy(key)
            peers[key]?.let { peers[key] = it.copy(legacy = false) }
        }
    }
}
