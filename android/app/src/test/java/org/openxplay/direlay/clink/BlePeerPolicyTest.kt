// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-16
package org.openxplay.direlay.clink

import org.junit.Assert.*
import org.junit.Test

class BlePeerPolicyTest {
    @Test fun defaultsToDualModeAndCccdWithIndependentPeers() {
        val policy = BlePeerPolicy()
        assertEquals(BlePeerPolicy.Choice(1002, false), policy.choice("first"))
        policy.cccdTimedOut("first", policy.choice("first"))
        policy.rejected("first", policy.choice("first"))
        assertEquals(BlePeerPolicy.Choice(1001, true), policy.choice("first"))
        assertEquals(BlePeerPolicy.Choice(1002, false), policy.choice("second"))
        policy.rejected("first", policy.choice("first"))
        assertEquals(BlePeerPolicy.Choice(1002, true), policy.choice("first"))
    }
    @Test fun remembersSuccessAcrossAddressesButOnlyWithinSession() {
        val policy = BlePeerPolicy()
        val key = policy.key("car-id", "old-address")
        assertEquals(key, policy.key("car-id", "new-address"))
        val choice = BlePeerPolicy.Choice(1001, true)
        policy.succeeded(key, choice)
        assertEquals(choice, policy.choice(policy.key("car-id", "new-address")))
        assertNotEquals(policy.key("", "one"), policy.key("", "two"))
        assertNotEquals(policy.key("one", "one"), policy.key("", "one"))
        assertEquals(BlePeerPolicy.Choice(), BlePeerPolicy().choice(key))
    }
    private class Store : BlePeerPolicy.Store, BlePeerPolicy.SubscriptionStore {
        val subscriptions = Subscriptions()
        override fun readLegacy(key: String) = subscriptions.readLegacy(key)
        override fun writeLegacy(key: String) = subscriptions.writeLegacy(key)
        override fun removeLegacy(key: String) = subscriptions.removeLegacy(key)
        val values = mutableMapOf<String, Int>()
        override fun read(key: String): Int? = values[key]
        override fun write(key: String, type: Int) { values[key] = type }
    }
    @Test fun successfulTypesPersistPerHashedPeerAcrossPolicyRecreation() {
        val store = Store()
        val first = BlePeerPolicy(store)
        val key = first.key("Personal car name", "address")
        assertTrue(key.matches(Regex("[0-9a-f]{64}")))
        assertFalse(key.contains("Personal"))
        first.rejected(key, first.choice(key))
        // A rejected choice must not overwrite the last successful type on disk.
        assertTrue(store.values.isEmpty())
        first.succeeded(key, first.choice(key))
        val next = BlePeerPolicy(store)
        assertEquals(1001, next.choice(next.key("Personal car name", "rotated")).requestType)
        assertEquals(1002, next.choice(next.key("Another car", "address")).requestType)
        next.rejected(key, next.choice(key))
        assertEquals(1002, next.choice(key).requestType)
        assertEquals(1001, BlePeerPolicy(store).choice(key).requestType)
        next.succeeded(key, next.choice(key))
        assertEquals(1002, BlePeerPolicy(store).choice(key).requestType)
    }
    @Test fun subscriptionTimeoutDoesNotChangeRememberedType() {
        val store = Store()
        val policy = BlePeerPolicy(store)
        policy.succeeded("car", BlePeerPolicy.Choice(1001))
        policy.cccdTimedOut("car", policy.choice("car"))
        assertEquals(1001, policy.choice("car").requestType)
        assertEquals(BlePeerPolicy.Choice(1001, false), BlePeerPolicy(store).choice("car"))
        store.values["bad"] = 99
        assertEquals(1002, BlePeerPolicy(store).choice("bad").requestType)
    }

    private class Subscriptions : BlePeerPolicy.SubscriptionStore {
        val legacy = mutableSetOf<String>()
        var reads = 0
        var writes = 0
        var removes = 0
        override fun readLegacy(key: String): Boolean { reads++; return key in legacy }
        override fun writeLegacy(key: String) { writes++; legacy.add(key) }
        override fun removeLegacy(key: String) { removes++; legacy.remove(key) }
    }

    @Test fun disabledSubscriptionPersistenceNeitherReadsNorWritesAndKeepsTodaysChoices() {
        val store = Store()
        val subscriptions = store.subscriptions.apply { legacy.add("car") }
        val policy = BlePeerPolicy(store)
        assertFalse(policy.choice("car").legacy)
        policy.cccdTimedOut("car", policy.choice("car"))
        policy.exchanged("car", policy.choice("car"))
        policy.succeeded("car", policy.choice("car"))
        policy.exchangeFailed("car")
        assertTrue(policy.choice("car").legacy)
        assertFalse(BlePeerPolicy(store).choice("car").legacy)
        assertEquals(0, subscriptions.reads + subscriptions.writes + subscriptions.removes)
        assertEquals(setOf("car"), subscriptions.legacy)
        assertEquals(1002, store.values["car"])
    }

    @Test fun enabledSubscriptionPersistenceLearnsOnlySuccessfulLegacyAndStartsFromIt() {
        val subscriptions = Subscriptions()
        val store = Store()
        val first = BlePeerPolicy(store, subscriptions)
        val key = first.key("car", "old-address")
        val other = first.key("other", "old-address")
        first.exchanged(key, first.choice(key))
        assertEquals(0, subscriptions.writes)
        first.cccdTimedOut(key, first.choice(key))
        assertTrue(subscriptions.legacy.isEmpty())
        first.exchanged(key, first.choice(key))
        first.succeeded(key, first.choice(key))
        assertEquals(setOf(key), subscriptions.legacy)
        val next = BlePeerPolicy(store, subscriptions)
        assertTrue(next.choice(next.key("car", "new-address")).legacy)
        assertFalse(next.choice(other).legacy)
        assertEquals(1, subscriptions.writes)
    }

    @Test fun failureAfterPersistedLegacyClearsDiskAndTheNextChoiceButKeepsRequestType() {
        val subscriptions = Subscriptions().apply { legacy.add("car") }
        val store = Store().apply { values["car"] = 1001 }
        val policy = BlePeerPolicy(store, subscriptions)
        val choice = policy.choice("car")
        assertEquals(BlePeerPolicy.Choice(1001, true), choice)
        policy.rejected("car", choice)
        policy.exchangeFailed("car")
        assertFalse(policy.choice("car").legacy)
        assertEquals(1002, policy.choice("car").requestType)
        assertEquals(BlePeerPolicy.Choice(1001, false), BlePeerPolicy(store, subscriptions).choice("car"))
        assertEquals(1, subscriptions.removes)
        policy.exchangeFailed("car")
        assertEquals(1, subscriptions.removes)
    }

}
