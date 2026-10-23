// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-14
package org.openxplay.direlay.clink

import org.openxplay.stream.clink.PinProvider
import java.util.concurrent.CompletableFuture
import java.util.concurrent.TimeUnit

/** The sheet observes one pending request; the wake-up callback never retains an Activity. */
class DialogPinProvider : PinProvider {
    class Request(val headUnitId: String, val confirmation: Boolean, private val afterResponse: (String?) -> Unit = {}, private val onCancel: () -> Unit = {}, val incorrect: Boolean = false) {
        internal val answer = CompletableFuture<String?>()
        fun respond(value: String?) { if (answer.complete(value)) afterResponse(value) }
        fun cancel() { respond(null); onCancel() }
    }
    @Volatile var onCancel: (() -> Unit)? = null
    @Volatile var onPending: (() -> Unit)? = null
    @Volatile var onSecret: (String) -> Unit = {}
    private var accepting = true
    private var lastPinRequest: Request? = null
    private var retryPin: Pair<String, String>? = null
    @Volatile var rejected = 0
        private set
    @Volatile var successes = 0
        private set
    @Synchronized fun reject(reconnect: () -> Unit) {
        val previous = lastPinRequest ?: return
        if (previous.answer.getNow(null) == null) return
        rejected++
        pending = Request(previous.headUnitId, false, afterResponse = { value ->
            if (value != null) onSecret(value)
            synchronized(this) {
                pending = null
                if (value != null) retryPin = previous.headUnitId to value
            }
            if (value != null) reconnect()
        }, onCancel = { cancel(); onCancel?.invoke() }, incorrect = true)
        runCatching { onPending?.invoke() }
    }
    @Synchronized fun paired() { if (lastPinRequest != null) successes++; pending = null; lastPinRequest = null; rejected = 0 }

    @Volatile var pending: Request? = null
        private set
    @Synchronized internal fun redactFailureMessage(text: String): String =
        listOfNotNull(lastPinRequest?.answer?.getNow(null), retryPin?.second)
            .filter { it.isNotEmpty() }.fold(text) { value, pin -> value.replace(pin, "[redacted]") }

    override fun requestPin(headUnitId: String): String? {
        synchronized(this) {
            retryPin?.takeIf { it.first == headUnitId }?.let {
                retryPin = null
                val request = Request(headUnitId, false, afterResponse = { value -> if (value != null) onSecret(value) })
                request.respond(it.second)
                lastPinRequest = request
                return it.second
            }
            retryPin = null
        }
        return ask(headUnitId, false)
    }
    override fun confirmPairing(headUnitId: String): Boolean = ask(headUnitId, true) == "approved"
    private fun ask(id: String, confirmation: Boolean): String? {
        val request = Request(id, confirmation, afterResponse = { value -> if (!confirmation && value != null) onSecret(value) },
            onCancel = { cancel(); onCancel?.invoke() })
        synchronized(this) {
            if (pending?.answer?.isDone == true) pending = null
            if (!accepting || pending != null) return null
            pending = request
            if (!confirmation) lastPinRequest = request
        }
        try {
            runCatching { onPending?.invoke() }
            return request.answer.get(120, TimeUnit.SECONDS)
        }
        catch (_: java.util.concurrent.TimeoutException) { return null }
        finally { synchronized(this) { if (pending === request) pending = null } }
    }
    @Synchronized fun cancel() { retire(); retryPin = null }
    @Synchronized fun retire() { accepting = false; pending?.respond(null); pending = null; lastPinRequest = null }
    @Synchronized fun resumeRequests() { accepting = true }
}
