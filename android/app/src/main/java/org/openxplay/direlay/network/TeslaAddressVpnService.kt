// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-22
package org.openxplay.direlay.network

import android.content.Context
import android.content.Intent
import android.net.VpnService
import android.os.Handler
import android.os.Looper
import android.os.ParcelFileDescriptor
import android.system.ErrnoException
import android.system.Os
import android.system.OsConstants
import android.system.StructPollfd
import org.openxplay.direlay.settings.TeslaAddressPolicy
import java.io.IOException
import java.net.Inet4Address
import java.net.NetworkInterface
import java.util.concurrent.atomic.AtomicLong

fun phoneIpv4Addresses(): Set<String> = NetworkInterface.getNetworkInterfaces()?.toList().orEmpty()
    .flatMap { it.inetAddresses.toList() }.filterIsInstance<Inet4Address>().mapNotNull { it.hostAddress }.toSet()

data class TeslaVpnStatus(val request: Long = 0, val address: String? = null,
    val active: Boolean = false, val failure: String? = null)

interface TeslaAddressVpn {
    fun start(address: String): Long
    fun stop()
    fun snapshot(): TeslaVpnStatus
}

/**
 * Adds one local address, without a VPN server, DNS or default route. The wildcard web listener
 * accepts packets addressed to this interface; anything routed into the TUN is discarded.
 * Its lifetime belongs to the relay's foreground session, and it never restarts by itself.
 */
class TeslaAddressVpnService : VpnService() {
    private val main = Handler(Looper.getMainLooper())
    private var tunnel: ParcelFileDescriptor? = null
    private var reader: Thread? = null
    @Volatile private var request = 0L

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        val next = intent?.getLongExtra(EXTRA_REQUEST, 0) ?: 0
        if (next == 0L || state.request != next) { stopSelf(startId); return START_NOT_STICKY }
        val address = intent?.getStringExtra(EXTRA_ADDRESS)
        if (request == next && tunnel != null) return START_NOT_STICKY
        releaseTunnel()
        request = next
        try {
            require(address != null && TeslaAddressPolicy.valid(address, phoneIpv4Addresses())) { "Invalid or already assigned Tesla address: $address" }
            check(VpnService.prepare(this) == null) { "VPN consent is required" }
            val established = Builder().setSession("OpenXPlay Tesla address").setMtu(1500)
                .addAddress(address, 32).addRoute(address, 32).setBlocking(false).establish()
                ?: throw IOException("Android did not establish the Tesla address VPN")
            val accepted = synchronized(stateLock) {
                if (state.request != next) false
                else { state = TeslaVpnStatus(next, address, active = true); true }
            }
            if (!accepted) { established.close(); stopSelf(startId); return START_NOT_STICKY }
            tunnel = established
            reader = Thread({ drain(established, next) }, "relay-tesla-address").apply { isDaemon = true; start() }
        } catch (failure: Exception) {
            failed(next, failure)
        }
        return START_NOT_STICKY
    }

    private fun drain(descriptor: ParcelFileDescriptor, id: Long) {
        val buffer = ByteArray(32768)
        val poll = StructPollfd().apply { fd = descriptor.fileDescriptor; events = OsConstants.POLLIN.toShort() }
        try {
            while (!Thread.currentThread().isInterrupted && state.request == id && state.active) {
                try {
                    if (Os.poll(arrayOf(poll), 500) == 0) continue
                    if ((poll.revents.toInt() and (OsConstants.POLLERR or OsConstants.POLLHUP or OsConstants.POLLNVAL)) != 0)
                        throw IOException("Tesla address tunnel closed")
                    if (Os.read(descriptor.fileDescriptor, buffer, 0, buffer.size) <= 0)
                        throw IOException("Tesla address tunnel ended")
                } catch (failure: ErrnoException) {
                    if (failure.errno != OsConstants.EAGAIN && failure.errno != OsConstants.EINTR) throw failure
                }
            }
        } catch (failure: Exception) {
            main.post { if (tunnel === descriptor && state.request == id && state.active) failed(id, failure) }
        } finally {
            runCatching { descriptor.close() }
        }
    }

    private fun failed(id: Long, failure: Exception) {
        synchronized(stateLock) {
            if (state.request == id) state = state.copy(active = false,
                failure = generateSequence<Throwable>(failure) { it.cause }
                    .take(5).joinToString("\nCaused by: ") { "${it.javaClass.simpleName}: ${it.message.orEmpty()}" })
        }
        releaseTunnel()
        stopSelf()
    }

    private fun releaseTunnel() {
        reader?.interrupt(); reader = null
        runCatching { tunnel?.close() }; tunnel = null
    }

    override fun onRevoke() {
        val revoked = request
        // Android may call this on a Binder thread; serialize it with service startup and destruction.
        main.post { if (request == revoked) failed(revoked, IOException("Android revoked the Tesla address VPN")) }
    }

    override fun onDestroy() {
        synchronized(stateLock) {
            if (state.request == request && state.active)
                state = state.copy(active = false, failure = "Tesla address VPN service stopped")
        }
        releaseTunnel()
        super.onDestroy()
    }

    companion object {
        private const val EXTRA_REQUEST = "tesla.request"
        private const val EXTRA_ADDRESS = "tesla.address"
        private val sequence = AtomicLong()
        private val stateLock = Any()
        @Volatile private var state = TeslaVpnStatus()

        fun controller(context: Context): TeslaAddressVpn = object : TeslaAddressVpn {
            override fun start(address: String): Long {
                val id = sequence.incrementAndGet()
                synchronized(stateLock) { state = TeslaVpnStatus(id, address) }
                try {
                    checkNotNull(context.startService(Intent(context, TeslaAddressVpnService::class.java)
                        .putExtra(EXTRA_REQUEST, id).putExtra(EXTRA_ADDRESS, address))) { "Android did not start the Tesla address service" }
                } catch (failure: Exception) {
                    synchronized(stateLock) {
                        if (state.request == id) state = TeslaVpnStatus(id, address,
                            failure = "${failure.javaClass.simpleName}: ${failure.message.orEmpty()}")
                    }
                    throw failure
                }
                return id
            }
            override fun stop() {
                synchronized(stateLock) { state = TeslaVpnStatus() }
                context.stopService(Intent(context, TeslaAddressVpnService::class.java))
            }
            override fun snapshot() = state
        }
    }
}
