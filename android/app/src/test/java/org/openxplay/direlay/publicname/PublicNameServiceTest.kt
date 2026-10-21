// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-20
package org.openxplay.direlay.publicname

import android.app.Application
import org.junit.Assert.*
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder
import org.junit.runner.RunWith
import org.openxplay.direlay.publicname.PublicNamePolicy.DAY
import org.openxplay.direlay.publicname.PublicNamePolicy.HOUR
import org.openxplay.direlay.publicname.PublicNamePolicy.MINUTE
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import java.security.PrivateKey
import java.security.cert.X509Certificate
import java.util.Random

/** The scheduler with a fake clock, a hand-driven worker and fake provider, CA and network. */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [28], manifest = Config.NONE, application = Application::class)
class PublicNameServiceTest {
    @get:Rule val temporary = TemporaryFolder()
    private val name = "myrelay.duckdns.org"
    private val token = "a1b2c3d4-0000-0000-0000-000000000000"
    private val start = 1_800_000_000_000L
    private var now = start
    private val worker = ManualWorker()
    private val provider = RecordingProvider()
    private val certificates = MemoryCertificates()
    private val issuer = FakeIssuer()
    private val internet = FakeInternet()
    private var addresses = listOf(AddressCandidate("10.0.0.7", false), AddressCandidate("192.168.43.1", true))
    private lateinit var store: PublicNameStore
    private lateinit var service: PublicNameService

    private inner class ManualWorker : PublicNameWorker {
        inner class Timer(val at: Long, val task: Runnable) { var cancelled = false }
        val queue = ArrayDeque<Runnable>()
        val timers = mutableListOf<Timer>()
        override fun execute(task: Runnable) { queue.add(task) }
        override fun schedule(delayMillis: Long, task: Runnable): () -> Unit {
            val timer = Timer(now + delayMillis, task)
            timers.add(timer)
            return { timer.cancelled = true }
        }
        override fun shutdown() { queue.clear(); timers.clear() }
        fun next(): Timer? = timers.filter { !it.cancelled }.minByOrNull { it.at }
    }

    private class RecordingProvider : DnsProvider {
        val events = mutableListOf<String>()
        var failure: PublicNameFailure? = null
        override val nameServers = listOf("ns1.test")
        override fun updateAddress(ipv4: String) { failure?.let { throw it }; events.add("address $ipv4") }
        override fun setChallenge(value: String): ChallengeRecord { events.add("set"); return ChallengeRecord() }
        override fun clearChallenge(record: ChallengeRecord) { events.add("clear") }
    }

    private inner class MemoryCertificates : AcmeCertificates {
        var installed: Pair<String, X509Certificate>? = null
        var served: String? = null
        override fun acmeCertificate(): AcmeCertificateInfo? = installed?.let { (name, leaf) ->
            AcmeCertificateInfo(name, leaf.notBefore.time, leaf.notAfter.time, AcmeCrypto.certId(leaf))
        }
        override fun installAcme(name: String, chain: List<X509Certificate>, key: PrivateKey) { installed = name to chain.first() }
        override fun removeAcme() { installed = null }
        override fun serveAcme(name: String?) { served = name }
    }

    private inner class FakeIssuer : CertificateIssuer {
        val ca = TestCa()
        val replaced = mutableListOf<String?>()
        var failure: PublicNameFailure? = null
        var window: RenewalWindow? = null
        /** The window applies to this certificate only, as a CA's suggestion would. */
        var windowFor: String? = null
        var windowRequests = 0
        /** Runs inside the attempt, before it pauses: simulates the user changing something meanwhile. */
        var during: ((Pause) -> Unit)? = null
        override fun issue(config: PublicNameConfig, provider: DnsProvider, replaces: String?, pause: Pause,
            beforeOrder: () -> Unit): IssuedCertificate {
            beforeOrder()
            replaced.add(replaces)
            during?.invoke(pause)
            failure?.let { throw it }
            val key = TestCa.keyPair()
            // Second-aligned like real certificates, and a 90-day lifetime.
            val leaf = ca.leaf(config.name, key.public, notBefore = now / 1000 * 1000 - 1000, notAfter = now / 1000 * 1000 + 90 * DAY)
            return IssuedCertificate(listOf(leaf, ca.certificate), key.private)
        }
        override fun renewalWindow(config: PublicNameConfig, certId: String): RenewalWindow? {
            windowRequests++
            return window?.takeIf { windowFor == null || windowFor == certId }
        }
    }

    private class FakeInternet : InternetAccess {
        var available = true
        var watcher: (() -> Unit)? = null
        override fun available() = available
        override fun watch(onValidated: () -> Unit) { watcher = onValidated }
        override fun stop() { watcher = null }
        fun validate() { available = true; watcher?.invoke() }
    }

    @Before fun create() {
        store = PublicNameStore(temporary.newFolder())
        service = PublicNameService(store, certificates, { provider }, issuer, internet, { addresses }, { now }, worker,
            sleep = { now += it }, random = Random(7))
    }

    private fun runQueued() { while (worker.queue.isNotEmpty()) worker.queue.removeFirst().run() }
    /** Moves the clock to the next timer and runs it; returns the delay since the previous time. */
    private fun fireNext(): Long {
        val timer = checkNotNull(worker.next()) { "nothing scheduled" }
        worker.timers.remove(timer)
        val delay = timer.at - now
        now = maxOf(now, timer.at)
        timer.task.run()
        runQueued()
        return delay
    }
    private fun fireUntil(time: Long) { while ((worker.next()?.at ?: Long.MAX_VALUE) <= time) fireNext(); now = maxOf(now, time) }
    private fun configure(enabled: Boolean = true, terms: Boolean = true, provider: DnsProviderId? = DnsProviderId.DUCKDNS,
        name: String = this.name, token: String = this.token): List<ConfigError> =
        service.save(PublicNameEdit(enabled, provider, name, token, terms)).also { runQueued() }
    private val status get() = service.status.value

    @Test fun offByDefaultRequestsNothingAndServesNothingNew() {
        service.start(); runQueued()
        assertEquals(PublicNameStatus(PublicNameState.OFF), status)
        assertFalse(service.active)
        assertTrue(provider.events.isEmpty())
        assertTrue(issuer.replaced.isEmpty())
        assertNull(certificates.served)
        assertNull("no network callback", internet.watcher)
        assertNull("nothing scheduled", worker.next())
        service.refreshAddress(); runQueued()
        assertTrue(provider.events.isEmpty())
    }

    @Test fun incompleteOrDisabledConfigurationRequestsNothing() {
        service.start(); runQueued()
        assertEquals(emptyList<ConfigError>(), configure(terms = false))
        assertEquals(PublicNameState.INCOMPLETE, status.state)
        assertEquals(name, status.name)
        configure(enabled = false)
        assertEquals(PublicNameState.OFF, status.state)
        assertTrue(provider.events.isEmpty())
        assertTrue(issuer.replaced.isEmpty())
    }

    @Test fun firstRunPublishesTheHotspotAddressThenGetsACertificate() {
        service.start(); runQueued()
        configure()
        assertEquals(listOf("address 192.168.43.1"), provider.events)
        assertEquals(listOf<String?>(null), issuer.replaced)
        assertEquals(PublicNameState.READY, status.state)
        assertEquals(name, status.trustedName)
        assertEquals("192.168.43.1", status.address)
        assertEquals(certificates.installed!!.second.notAfter.time, status.notAfter)
        assertEquals(name, certificates.served)
        assertEquals("the 12 h check", now + 12 * HOUR, worker.next()!!.at)
        assertEquals(listOf(start), store.record().orders(name))
    }

    @Test fun addressIsRepublishedOnlyWhenItChangesOrAfter24Hours() {
        configure()
        fireUntil(start + 23 * HOUR)
        assertEquals(1, provider.events.count { it.startsWith("address") })
        fireUntil(start + 25 * HOUR)
        assertEquals(2, provider.events.count { it.startsWith("address") })
        service.refreshAddress(); runQueued()
        assertEquals("unchanged address", 2, provider.events.count { it.startsWith("address") })
        addresses = listOf(AddressCandidate("10.0.0.7", false))
        provider.failure = PublicNameFailure(PublicNameError.PROVIDER_UNREACHABLE)
        service.refreshAddress()
        assertNull("the name stops pointing here at once", status.trustedName)
        runQueued()
        provider.failure = null
        assertEquals(PublicNameState.ERROR, status.state)
        assertNull(status.trustedName)
        fireNext()
        assertEquals("address 10.0.0.7", provider.events.last())
        assertEquals(PublicNameState.READY, status.state)
        assertEquals(name, status.trustedName)
        assertEquals(1, issuer.replaced.size)
    }

    @Test fun failuresBackOffOneFiveThirtyMinutesTwoAndSixHoursThenDaily() {
        provider.failure = PublicNameFailure(PublicNameError.TOKEN_REJECTED)
        configure()
        assertEquals(PublicNameState.ERROR, status.state)
        assertEquals(PublicNameError.TOKEN_REJECTED, status.error)
        assertEquals(now + MINUTE, status.nextAttempt)
        val delays = (1..7).map { fireNext() }
        assertEquals(listOf(MINUTE, 5 * MINUTE, 30 * MINUTE, 2 * HOUR, 6 * HOUR, DAY, DAY), delays)
        // An unchanged address does not retry early; a new address resets the backoff.
        service.refreshAddress(); runQueued()
        assertEquals(now + DAY, worker.next()!!.at)
        addresses = listOf(AddressCandidate("10.0.0.9", false))
        service.refreshAddress(); runQueued()
        assertEquals(now + MINUTE, status.nextAttempt)
        assertTrue(issuer.replaced.isEmpty())
    }

    @Test fun withoutInternetItWaitsAndRetriesWhenAValidatedNetworkAppears() {
        internet.available = false
        provider.failure = PublicNameFailure(PublicNameError.PROVIDER_UNREACHABLE)
        configure()
        assertEquals(PublicNameState.WAITING_NETWORK, status.state)
        assertEquals(now + MINUTE, status.nextAttempt)
        provider.failure = null
        internet.validate(); runQueued()
        assertEquals(PublicNameState.READY, status.state)
    }

    @Test fun localGuardAllowsThreeOrdersPerNamePerDay() {
        issuer.failure = PublicNameFailure(PublicNameError.ACME_REJECTED, detail = "boom")
        configure()
        fireNext(); fireNext()
        assertEquals(3, issuer.replaced.size)
        assertEquals("boom", status.detail)
        fireNext()
        assertEquals("the fourth order is not sent", 3, issuer.replaced.size)
        assertEquals(PublicNameState.ERROR, status.state)
        assertEquals(PublicNameError.ACME_RATE_LIMITED, status.error)
        assertEquals(start + DAY, status.nextAttempt)
        // The address still follows the network, and Request certificate now does not skip the guard.
        addresses = listOf(AddressCandidate("10.0.0.9", false))
        service.refreshAddress(); runQueued()
        assertEquals("address 10.0.0.9", provider.events.last())
        service.renewNow(); runQueued()
        assertEquals(3, issuer.replaced.size)
        assertEquals(start + DAY, status.nextAttempt)
        issuer.failure = null
        fireUntil(start + DAY)
        assertEquals(4, issuer.replaced.size)
        assertEquals(PublicNameState.READY, status.state)
    }

    @Test fun theCasRetryAfterIsObeyedEvenByRequestCertificateNow() {
        issuer.failure = PublicNameFailure(PublicNameError.ACME_RATE_LIMITED, retryAfterMillis = 3 * HOUR)
        configure()
        assertEquals(PublicNameError.ACME_RATE_LIMITED, status.error)
        assertEquals(start + 3 * HOUR, status.nextAttempt)
        service.renewNow(); runQueued()
        assertEquals(1, issuer.replaced.size)
        assertEquals("the address is refreshed", 2, provider.events.count { it.startsWith("address") })
        assertEquals(PublicNameError.ACME_RATE_LIMITED, status.error)
        assertEquals(start + 3 * HOUR, status.nextAttempt)
        issuer.failure = null
        fireUntil(start + 3 * HOUR)
        assertEquals(2, issuer.replaced.size)
        assertEquals(PublicNameState.READY, status.state)
    }

    @Test fun requestCertificateNowWithAValidCertificateOnlyRefreshesTheAddress() {
        configure()
        service.renewNow(); runQueued()
        assertEquals(listOf("address 192.168.43.1", "address 192.168.43.1"), provider.events)
        assertEquals(1, issuer.replaced.size)
        assertEquals(PublicNameState.READY, status.state)
    }

    @Test fun requestCertificateNowRetriesAtOnceAfterAPlainFailure() {
        provider.failure = PublicNameFailure(PublicNameError.PROVIDER_UNREACHABLE)
        configure()
        fireNext(); fireNext()
        provider.failure = null
        service.renewNow(); runQueued()
        assertEquals(PublicNameState.READY, status.state)
    }

    @Test fun withoutAriRenewalStartsWhenAThirdOfTheLifetimeRemains() {
        configure()
        val notBefore = certificates.installed!!.second.notBefore.time
        val notAfter = certificates.installed!!.second.notAfter.time
        val renewal = notAfter - (notAfter - notBefore) / 3
        fireUntil(renewal - 1)
        assertEquals(1, issuer.replaced.size)
        fireUntil(renewal + 12 * HOUR)
        assertEquals(listOf<String?>(null, null), issuer.replaced)
        assertEquals(PublicNameState.READY, status.state)
    }

    @Test fun ariWindowStartsRenewalEarlierAndNamesTheReplacedCertificate() {
        configure()
        val first = certificates.installed!!.second
        issuer.window = RenewalWindow(start + 30 * DAY, start + 31 * DAY, null)
        issuer.windowFor = AcmeCrypto.certId(first)
        fireUntil(start + 29 * DAY)
        assertEquals(1, issuer.replaced.size)
        assertTrue("ARI is polled at most every 12 h", issuer.windowRequests in 1..60)
        fireUntil(start + 31 * DAY + 12 * HOUR)
        assertEquals(listOf(null, AcmeCrypto.certId(first)), issuer.replaced)
    }

    @Test fun aChangeDuringAnAttemptCancelsItBeforeAnythingIsInstalled() {
        configure(enabled = false)
        issuer.during = { pause ->
            issuer.during = null
            service.save(PublicNameEdit(true, DnsProviderId.DUCKDNS, "other.duckdns.org", "", true))
            pause.pause(1_000)
        }
        configure()
        assertEquals("the stale attempt installed nothing; the new one did", "other.duckdns.org", certificates.installed?.first)
        assertEquals(2, issuer.replaced.size)
        assertEquals(PublicNameState.READY, status.state)
        assertEquals("other.duckdns.org", status.trustedName)
    }

    @Test fun disablingStopsRequestsAndServesOnlyTodaysCertificate() {
        configure()
        configure(enabled = false)
        assertEquals(PublicNameState.OFF, status.state)
        assertNull(certificates.served)
        assertNull(internet.watcher)
        assertNotNull("the certificate is kept for later", certificates.installed)
        val events = provider.events.size
        while (worker.next() != null) fireNext()
        assertEquals(events, provider.events.size)
    }

    @Test fun removeDeletesConfigurationTokenAndCertificateButNeverTheARecord() {
        configure()
        store.accountKey()
        service.remove(); runQueued()
        assertEquals(PublicNameState.OFF, status.state)
        assertNull(store.config())
        assertNull(store.accountKey())
        assertNull(certificates.installed)
        assertNull(certificates.served)
        assertEquals(PublicNameView(revision = service.view.value.revision), service.view.value)
        assertEquals("no request to clear the address", listOf("address 192.168.43.1"), provider.events)
    }

    @Test fun savingKeepsTheTokenPrivateAndChecksTheFields() {
        assertEquals(listOf(ConfigError("name", ConfigProblem.INVALID)), configure(name = "myrelay.example.org"))
        assertEquals(listOf(ConfigError("token", ConfigProblem.INVALID)), configure(token = "not-a-uuid"))
        assertNull(store.config())
        configure(enabled = false)
        configure(enabled = false, token = "")
        assertEquals("an empty token keeps the saved one", token, store.config()!!.token)
        assertTrue(service.view.value.tokenSet)
        assertFalse(service.view.value.toString().contains(token))
        configure(enabled = false, provider = DnsProviderId.DYNV6, name = "Relay.Dynv6.NET", token = " dynv6-token ")
        assertEquals(PublicNameConfig(false, DnsProviderId.DYNV6, "relay.dynv6.net", "dynv6-token", true), store.config())
        // Back to DuckDNS without a new token: the kept dynv6 token does not fit, and is reported instead of kept silently.
        assertEquals(listOf(ConfigError("token", ConfigProblem.INVALID)),
            configure(enabled = false, provider = DnsProviderId.DUCKDNS, name = "x.duckdns.org", token = ""))
        assertEquals("dynv6-token", store.config()!!.token)
        val imported = service.import("""{"version":1,"provider":"duckdns","name":"x.duckdns.org","token":"$token","agreeLetsEncryptTerms":true}""".toByteArray())
        assertTrue(imported is ConfigParse.Valid)
        assertEquals(PublicNameConfig(true, DnsProviderId.DUCKDNS, "x.duckdns.org", token, true), store.config())
        assertTrue(service.import("""{"version":1}""".toByteArray()) is ConfigParse.Invalid)
        assertEquals("a rejected file changes nothing", "x.duckdns.org", store.config()!!.name)
        runQueued()
        assertFalse(status.toString().contains(token))
    }

    @Test fun noAddressMeansNoTrustedName() {
        addresses = listOf(AddressCandidate("100.64.0.1", true))
        configure()
        assertEquals(PublicNameState.ERROR, status.state)
        assertEquals(PublicNameError.NO_ADDRESS, status.error)
        assertNull(status.trustedName)
        assertTrue("nothing is published", provider.events.isEmpty())
        assertEquals("the certificate does not need the address", 1, issuer.replaced.size)
    }
}
