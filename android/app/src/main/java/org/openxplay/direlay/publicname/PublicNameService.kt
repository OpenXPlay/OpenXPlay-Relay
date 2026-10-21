// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-20
package org.openxplay.direlay.publicname

import android.content.Context
import android.net.ConnectivityManager
import android.net.Network
import android.net.NetworkCapabilities
import android.util.Log
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import java.security.PrivateKey
import java.security.SecureRandom
import java.security.cert.X509Certificate
import java.util.Random
import java.util.concurrent.Executors
import java.util.concurrent.RejectedExecutionException
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicBoolean
import java.util.concurrent.atomic.AtomicLong

/** The status line states shared with the Linux relay. */
enum class PublicNameState(val key: String) {
    OFF("off"), INCOMPLETE("incomplete"), WAITING_NETWORK("waiting-network"), UPDATING_ADDRESS("updating-address"),
    REQUESTING_CERTIFICATE("requesting-certificate"), READY("ready"), ERROR("error"),
}

/**
 * The status line. [trustedName] is set while the certificate for the name is valid and the name points
 * at the current head-unit address; only then is `https://<name>:8443/` offered to the head unit.
 */
data class PublicNameStatus(val state: PublicNameState, val name: String? = null, val address: String? = null,
    val notAfter: Long? = null, val error: PublicNameError? = null, val detail: String? = null,
    val nextAttempt: Long? = null, val trustedName: String? = null)

/** What the settings screen may show; never the token itself. [revision] changes with every saved change. */
data class PublicNameView(val configured: Boolean = false, val enabled: Boolean = false, val provider: DnsProviderId? = null,
    val name: String = "", val tokenSet: Boolean = false, val agreeTerms: Boolean = false, val revision: Long = 0)

/** A settings-screen change; an empty [token] keeps the saved one. */
class PublicNameEdit(val enabled: Boolean, val provider: DnsProviderId?, val name: String, val token: String, val agreeTerms: Boolean) {
    override fun toString() = "PublicNameEdit(enabled=$enabled, provider=${provider?.key}, name=$name)"
}

data class AcmeCertificateInfo(val name: String, val notBefore: Long, val notAfter: Long, val certId: String?)

/** The ACME certificate source in the TLS store, separate from imported and generated material. */
interface AcmeCertificates {
    /** Null when there is none or it fails validation. */
    fun acmeCertificate(): AcmeCertificateInfo?
    fun installAcme(name: String, chain: List<X509Certificate>, key: PrivateKey)
    fun removeAcme()
    /** Serve the ACME certificate for SNI [name] while it is valid; null serves only today's certificate. */
    fun serveAcme(name: String?)
}

interface InternetAccess {
    fun available(): Boolean
    /** Calls [onValidated] whenever a validated default network appears, until [stop]. */
    fun watch(onValidated: () -> Unit)
    fun stop()
}

/** The service's single worker thread; tests run its tasks by hand. */
interface PublicNameWorker {
    fun execute(task: Runnable)
    /** Runs [task] after [delayMillis]; the returned function cancels it. */
    fun schedule(delayMillis: Long, task: Runnable): () -> Unit
    fun shutdown()
}

class ExecutorWorker : PublicNameWorker {
    private val executor = Executors.newSingleThreadScheduledExecutor { task ->
        Thread(task, "direlay-public-name").apply { isDaemon = true }
    }
    override fun execute(task: Runnable) = executor.execute(task)
    override fun schedule(delayMillis: Long, task: Runnable): () -> Unit {
        val future = executor.schedule(task, delayMillis, TimeUnit.MILLISECONDS)
        return { future.cancel(false) }
    }
    override fun shutdown() { executor.shutdownNow() }
}

/**
 * Publishes the head-unit address under the public name and keeps a Let's Encrypt certificate for it.
 * Everything runs on one worker thread; a configuration change cancels a running attempt at its next wait.
 * With no configuration, or with it disabled, nothing is requested and only today's certificate is served.
 */
class PublicNameService(
    private val store: PublicNameStore,
    private val certificates: AcmeCertificates,
    private val providers: (PublicNameConfig) -> DnsProvider,
    private val issuer: CertificateIssuer,
    private val internet: InternetAccess,
    private val addressSource: () -> Collection<AddressCandidate>,
    private val clock: () -> Long = System::currentTimeMillis,
    private val worker: PublicNameWorker = ExecutorWorker(),
    private val sleep: (Long) -> Unit = Thread::sleep,
    private val random: Random = SecureRandom(),
) : AutoCloseable {
    private val mutableStatus = MutableStateFlow(PublicNameStatus(PublicNameState.OFF))
    val status: StateFlow<PublicNameStatus> = mutableStatus.asStateFlow()
    private val mutableView = MutableStateFlow(PublicNameView())
    val view: StateFlow<PublicNameView> = mutableView.asStateFlow()

    private val generation = AtomicLong()
    /** Store and certificate changes of a run happen under this lock, and only while its generation is current. */
    private val lock = Any()
    private val queued = AtomicBoolean()
    private val resetBackoff = AtomicBoolean()
    private val bypassWait = AtomicBoolean()
    private val forcePublish = AtomicBoolean()
    @Volatile private var closed = false
    @Volatile private var networkBlocked = false
    @Volatile private var address: String? = null
    /** True while a configuration is enabled; the runtime then reports address changes. */
    @Volatile var active = false
        private set

    private data class Published(val generation: Long, val key: String, val address: String, val at: Long)
    /** A Retry-After or the local order guard for one provider and name; [certificateOnly] still allows address updates. */
    private class Hold(val key: String, val until: Long, val failure: PublicNameFailure, val certificateOnly: Boolean)
    @Volatile private var published: Published? = null
    // Worker thread only.
    private var failures = 0
    private var retryAt: Long? = null
    private var hold: Hold? = null
    private var timer: (() -> Unit)? = null

    private class Cancelled : RuntimeException()

    fun start() {
        publishView(store.config())
        trigger()
    }

    /** Called by the runtime's network tick; only a changed address starts an attempt. */
    fun refreshAddress() {
        if (!active) return
        val chosen = PublicNamePolicy.chooseAddress(addressSource())
        if (chosen == address) return
        address = chosen
        // The name points at the old address until the update; stop offering it at once.
        if (published?.address != chosen) mutableStatus.update { it.copy(trustedName = null) }
        if (chosen != null) resetBackoff.set(true)
        trigger()
    }

    /** Validates and saves the settings screen's fields; returns per-key problems instead of saving. */
    fun save(edit: PublicNameEdit): List<ConfigError> {
        val name = PublicNameRules.normalizeName(edit.name.trim())
        val token = edit.token.trim()
        synchronized(lock) {
            val current = store.config()
            val kept = token.ifEmpty { current?.token.orEmpty() }
            val errors = buildList {
                if (name.isNotEmpty() && !PublicNameRules.nameValid(edit.provider, name)) add(ConfigError(PublicNameFile.NAME, ConfigProblem.INVALID))
                // A saved token that does not fit a newly chosen provider is reported, never silently kept or dropped.
                if (kept.isNotEmpty() && !PublicNameRules.tokenValid(edit.provider, kept)) add(ConfigError(PublicNameFile.TOKEN, ConfigProblem.INVALID))
            }
            if (errors.isNotEmpty()) return errors
            store.save(PublicNameConfig(edit.enabled, edit.provider, name, kept, edit.agreeTerms, current?.acme ?: AcmeEnvironment.PRODUCTION))
            changed()
        }
        return emptyList()
    }

    /** An imported file replaces the whole configuration; nothing is saved when it has problems. */
    fun import(bytes: ByteArray): ConfigParse {
        val parsed = PublicNameFile.parse(bytes)
        if (parsed is ConfigParse.Valid) synchronized(lock) { store.save(parsed.config); changed() }
        return parsed
    }

    /** Deletes the configuration, token, account key and ACME certificate; the provider's A record stays. */
    fun remove() {
        synchronized(lock) {
            generation.incrementAndGet()
            certificates.serveAcme(null)
            store.remove()
            certificates.removeAcme()
            publishView(null)
            resetBackoff.set(true)
        }
        trigger()
    }

    /**
     * "Request certificate now" retries now: backoff reset, address refreshed, certificate checked.
     * It never skips the local order guard or a Retry-After; a valid certificate that is not due stays.
     */
    fun renewNow() {
        forcePublish.set(true)
        bypassWait.set(true)
        resetBackoff.set(true)
        trigger()
    }

    private fun networkAvailable() {
        if (!networkBlocked) return
        bypassWait.set(true)
        trigger()
    }

    private fun key(config: PublicNameConfig) = "${config.provider?.key}:${config.name}"

    private fun changed() {
        generation.incrementAndGet()
        resetBackoff.set(true)
        publishView(store.config())
        trigger()
    }

    private fun publishView(config: PublicNameConfig?) {
        active = config?.enabled == true
        mutableView.value = PublicNameView(config != null, config?.enabled == true, config?.provider, config?.name.orEmpty(),
            config?.token?.isNotEmpty() == true, config?.agreeTerms == true, generation.get())
    }

    private fun trigger() {
        if (closed || !queued.compareAndSet(false, true)) return
        try { worker.execute { queued.set(false); safeCycle() } } catch (_: RejectedExecutionException) { queued.set(false) }
    }

    private fun schedule(at: Long) {
        timer?.invoke()
        timer = try { worker.schedule((at - clock()).coerceAtLeast(0)) { safeCycle() } } catch (_: RejectedExecutionException) { null }
    }

    /** A store that cannot be read must not end the schedule silently; the next trigger or the hourly retry tries again. */
    private fun safeCycle() {
        try { cycle() } catch (failure: RuntimeException) {
            Log.w(TAG, "Public name check failed: ${failure.javaClass.simpleName}")
            if (!closed) schedule(clock() + PublicNamePolicy.HOUR)
        } catch (failure: java.io.IOException) {
            Log.w(TAG, "Public name check failed: ${failure.javaClass.simpleName}")
            if (!closed) schedule(clock() + PublicNamePolicy.HOUR)
        }
    }

    /** One attempt: address first, then the certificate. Internal for tests, which run it on their own thread. */
    internal fun cycle() {
        if (closed) return
        timer?.invoke(); timer = null
        val run = generation.get()
        val config = store.config()
        if (config == null || !config.enabled) { off(); return }
        active = true
        internet.watch(::networkAvailable)
        certificates.serveAcme(config.name.ifEmpty { null })
        val target = PublicNamePolicy.chooseAddress(addressSource())
        address = target
        if (!config.complete) { set(null, PublicNameStatus(PublicNameState.INCOMPLETE, config.name.ifEmpty { null })); return }
        val key = key(config)
        val now = clock()
        if (resetBackoff.getAndSet(false)) { failures = 0; retryAt = null }
        val bypass = bypassWait.getAndSet(false)
        val held = hold?.takeIf { it.key == key && now < it.until }
        hold = held
        retryAt?.let { if (now < it && !bypass) { schedule(it); return } }
        // The provider asked to wait: nothing goes to it until then, whatever triggered this attempt.
        if (held != null && !held.certificateOnly) { settle(config, Long.MAX_VALUE, held); return }
        try {
            val provider = providers(config)
            if (target != null) publish(config, provider, target, run)
            // The CA's Retry-After or the local order guard: the address stays current, no new order.
            val check = if (held != null) Long.MAX_VALUE else certificate(config, provider, run)
            failures = 0; retryAt = null; networkBlocked = false
            settle(config, check, held)
        } catch (_: Cancelled) { trigger() }
          catch (failure: PublicNameFailure) { if (stale(run)) trigger() else fail(config, failure) }
          catch (failure: RuntimeException) {
              if (stale(run)) trigger() else {
                  Log.w(TAG, "Public name attempt failed: ${failure.javaClass.simpleName}")
                  fail(config, PublicNameFailure(PublicNameError.PROVIDER_UNREACHABLE))
              }
          }
    }

    /** Publishes [target] at start, when it or the configuration changed, when asked, and after 24 h. */
    private fun publish(config: PublicNameConfig, provider: DnsProvider, target: String, run: Long) {
        val previous = published
        val forced = forcePublish.getAndSet(false)
        if (!forced && previous != null && previous.generation == run && previous.address == target &&
            clock() - previous.at < PublicNamePolicy.REPUBLISH_AFTER) return
        set(config, PublicNameStatus(PublicNameState.UPDATING_ADDRESS, config.name, target))
        provider.updateAddress(target)
        published = Published(run, key(config), target, clock())
        Log.i(TAG, "Public name: address published")
    }

    private fun validCertificate(config: PublicNameConfig, now: Long): AcmeCertificateInfo? =
        certificates.acmeCertificate()?.takeIf { it.name == config.name && now >= it.notBefore && now < it.notAfter }

    /** Requests a certificate when needed and allowed; returns when the next certificate check is due. */
    private fun certificate(config: PublicNameConfig, provider: DnsProvider, run: Long): Long {
        val now = clock()
        val current = validCertificate(config, now)
        val plan = current?.let { renewalPlan(config, it, now, run) }
        val renewal = current?.let { PublicNamePolicy.renewalTime(it.notBefore, it.notAfter, plan?.renewAt) }
        val due = renewal != null && now >= renewal
        if (renewal != null && !due) return renewal
        when (val decision = PublicNamePolicy.orderDecision(store.record().orders(config.name), current?.notAfter, due, now)) {
            PublicNamePolicy.OrderDecision.KeepCurrent -> return checkNotNull(renewal)
            is PublicNamePolicy.OrderDecision.Wait ->
                if (current != null) return decision.at else throw PublicNameFailure(PublicNameError.ACME_RATE_LIMITED, decision.at - now)
            PublicNamePolicy.OrderDecision.Allowed -> Unit
        }
        set(config, PublicNameStatus(PublicNameState.REQUESTING_CERTIFICATE, config.name, address))
        val replaces = current?.certId?.takeIf { plan?.renewAt != null }
        val issued = issuer.issue(config, provider, replaces, pause(run)) {
            // Counted before the request leaves, so an order the CA created without answering still counts.
            guarded(run) {
                val record = store.record()
                val at = clock()
                store.saveRecord(record.copy(orders = record.orders.filter { at - it.second < PublicNamePolicy.DAY } + (config.name to at)))
            }
        }
        guarded(run) {
            certificates.installAcme(config.name, issued.chain, issued.key)
            store.saveRecord(store.record().copy(ari = null))
        }
        val leaf = issued.chain.first()
        Log.i(TAG, "Public name: certificate installed, valid until ${java.time.Instant.ofEpochMilli(leaf.notAfter.time)}")
        return PublicNamePolicy.renewalTime(leaf.notBefore.time, leaf.notAfter.time, null)
    }

    /** ARI polled no more often than its Retry-After allows; failures keep the previous plan. */
    private fun renewalPlan(config: PublicNameConfig, certificate: AcmeCertificateInfo, now: Long, run: Long): AriState? {
        val certId = certificate.certId ?: return null
        val record = store.record()
        val previous = record.ari?.takeIf { it.certId == certId }
        if (previous != null && now < previous.nextPoll) return previous.takeIf { it.renewAt != null }
        val window = try { issuer.renewalWindow(config, certId) } catch (_: PublicNameFailure) {
            return previous?.takeIf { it.renewAt != null }
        }
        val plan = if (window == null) AriState(certId, null, null, null, now + PublicNamePolicy.CHECK_INTERVAL)
            else PublicNamePolicy.ariPlan(certId, window, previous, now, random)
        guarded(run) { store.saveRecord(store.record().copy(ari = plan)) }
        return plan.takeIf { it.renewAt != null }
    }

    /** The status after a completed attempt and the next check: every 12 h, at renewal, before the 24 h republish. */
    private fun settle(config: PublicNameConfig, certificateCheck: Long, held: Hold?) {
        val now = clock()
        val target = address
        val refresh = published?.takeIf { it.address == target }?.let { it.at + PublicNamePolicy.REPUBLISH_AFTER } ?: Long.MAX_VALUE
        val next = minOf(now + PublicNamePolicy.CHECK_INTERVAL, certificateCheck, refresh)
        val certificate = validCertificate(config, now)
        when {
            held != null -> {
                set(config, PublicNameStatus(PublicNameState.ERROR, config.name, target, certificate?.notAfter, held.failure.error,
                    held.failure.detail, held.until))
                schedule(minOf(next, held.until).coerceAtLeast(now + PublicNamePolicy.MINUTE))
            }
            target == null -> {
                set(config, PublicNameStatus(PublicNameState.ERROR, config.name, null, certificate?.notAfter, PublicNameError.NO_ADDRESS,
                    nextAttempt = now + PublicNamePolicy.MINUTE))
                schedule(now + PublicNamePolicy.MINUTE)
            }
            certificate == null -> {
                set(config, PublicNameStatus(PublicNameState.ERROR, config.name, target, error = PublicNameError.ACME_REJECTED,
                    detail = "certificate not usable", nextAttempt = now + PublicNamePolicy.HOUR))
                schedule(now + PublicNamePolicy.HOUR)
            }
            else -> {
                set(config, PublicNameStatus(PublicNameState.READY, config.name, target, certificate.notAfter))
                schedule(next.coerceAtLeast(now + PublicNamePolicy.MINUTE))
            }
        }
    }

    private fun fail(config: PublicNameConfig, failure: PublicNameFailure) {
        val now = clock()
        val retryAfter = failure.retryAfterMillis
        val at: Long
        if (retryAfter != null) {
            at = now + retryAfter.coerceAtLeast(PublicNamePolicy.MINUTE)
            hold = Hold(key(config), at, failure, certificateOnly = failure.error == PublicNameError.ACME_RATE_LIMITED)
        } else {
            failures++
            at = now + PublicNamePolicy.backoff(failures)
            retryAt = at
        }
        networkBlocked = failure.error == PublicNameError.PROVIDER_UNREACHABLE
        val offline = networkBlocked && !internet.available()
        set(config, if (offline) PublicNameStatus(PublicNameState.WAITING_NETWORK, config.name, address, nextAttempt = at)
            else PublicNameStatus(PublicNameState.ERROR, config.name, address, error = failure.error, detail = failure.detail, nextAttempt = at))
        Log.i(TAG, "Public name: ${if (offline) "waiting for internet" else failure.error.key}, next try in ${(at - now) / 1000} s")
        schedule(at)
    }

    /** Requests stop and only today's certificate is served; a Retry-After for the name is still obeyed later. */
    private fun off() {
        active = false
        internet.stop()
        certificates.serveAcme(null)
        published = null; address = null; failures = 0; retryAt = null; networkBlocked = false
        forcePublish.set(false); bypassWait.set(false)
        set(null, PublicNameStatus(PublicNameState.OFF))
    }

    private fun set(config: PublicNameConfig?, status: PublicNameStatus) {
        mutableStatus.value = status.copy(trustedName = config?.let(::trustedName))
    }

    /** The name while its certificate is valid and the address it points at is the current one. */
    private fun trustedName(config: PublicNameConfig): String? {
        val target = address ?: return null
        published?.takeIf { it.key == key(config) && it.address == target } ?: return null
        return config.name.takeIf { validCertificate(config, clock()) != null }
    }

    private fun stale(run: Long) = closed || generation.get() != run

    private inline fun guarded(run: Long, block: () -> Unit) = synchronized(lock) {
        if (stale(run)) throw Cancelled()
        block()
    }

    /** Waits in short slices so a configuration change or shutdown ends the attempt promptly. */
    private fun pause(run: Long) = Pause { millis ->
        var left = millis
        while (left > 0) {
            if (stale(run)) throw Cancelled()
            val slice = minOf(left, 500L)
            try { sleep(slice) } catch (_: InterruptedException) { throw Cancelled() }
            left -= slice
        }
        if (stale(run)) throw Cancelled()
    }

    override fun close() {
        closed = true
        generation.incrementAndGet()
        internet.stop()
        worker.shutdown()
    }

    companion object {
        private const val TAG = "OpenXPlay Relay"

        fun create(context: Context, certificates: AcmeCertificates, version: String,
            addressSource: () -> Collection<AddressCandidate>): PublicNameService {
            val store = PublicNameStore(context.noBackupFilesDir)
            val http = UrlConnectionHttp(UrlConnectionHttp.userAgent(version))
            return PublicNameService(store, certificates, { dnsProvider(it, http) }, AcmeIssuer(http, store, UdpTxtLookup()),
                AndroidInternetAccess(context.applicationContext), addressSource)
        }
    }
}

/** The default network's VALIDATED capability; a newly validated network retries a network failure at once. */
class AndroidInternetAccess(context: Context) : InternetAccess {
    private val connectivity = context.getSystemService(ConnectivityManager::class.java)
    private var callback: ConnectivityManager.NetworkCallback? = null

    override fun available(): Boolean = runCatching {
        connectivity.getNetworkCapabilities(connectivity.activeNetwork)?.hasCapability(NetworkCapabilities.NET_CAPABILITY_VALIDATED) == true
    }.getOrDefault(false)

    @Synchronized override fun watch(onValidated: () -> Unit) {
        if (callback != null) return
        val watcher = object : ConnectivityManager.NetworkCallback() {
            @Volatile private var validated = available()
            override fun onCapabilitiesChanged(network: Network, capabilities: NetworkCapabilities) {
                val now = capabilities.hasCapability(NetworkCapabilities.NET_CAPABILITY_VALIDATED)
                if (now && !validated) onValidated()
                validated = now
            }
            override fun onLost(network: Network) { validated = false }
        }
        if (runCatching { connectivity.registerDefaultNetworkCallback(watcher) }.isSuccess) callback = watcher
    }

    @Synchronized override fun stop() {
        callback?.let { runCatching { connectivity.unregisterNetworkCallback(it) } }
        callback = null
    }
}
