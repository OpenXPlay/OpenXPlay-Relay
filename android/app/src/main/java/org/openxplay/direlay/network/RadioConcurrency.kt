// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-29
package org.openxplay.direlay.network

import android.content.Context
import android.net.wifi.WifiManager
import android.os.Build

/** Whether this phone keeps Wi-Fi on while its own hotspot runs. */
enum class RadioConcurrency { SUPPORTED, EXCLUSIVE, UNKNOWN }

/** What this phone did with its radios; the newest observation outranks the platform's answer. */
data class RadioObservation(val exclusive: Boolean, val atMillis: Long, val evidence: String)

/**
 * Learns from radio readings. Wi-Fi switching off within [window] of the hotspot starting, or back on
 * after it stopped, means the system keeps only one of them; both on for [window] means it keeps both.
 * The platform answer covers the time before the first observation.
 */
class RadioConcurrencyObserver(val platform: Boolean?, observed: RadioObservation? = null,
    private val window: Long = 5_000, private val save: (RadioObservation) -> Unit = {}) {
    @Volatile var observation: RadioObservation? = observed
        private set
    val concurrency: RadioConcurrency get() = observation?.let { if (it.exclusive) RadioConcurrency.EXCLUSIVE else RadioConcurrency.SUPPORTED }
        ?: when (platform) { true -> RadioConcurrency.SUPPORTED; false -> RadioConcurrency.EXCLUSIVE; null -> RadioConcurrency.UNKNOWN }
    private var wifi: Boolean? = null
    private var hotspot: Boolean? = null
    private var wifiOffAt: Long? = null
    private var wifiOnAt: Long? = null
    private var hotspotOnAt: Long? = null
    private var hotspotOffAt: Long? = null
    private var bothSince: Long? = null

    /** [now] is a monotonic clock, [wall] the time to keep. Returns the observation this reading changed. */
    @Synchronized fun observe(wifiOn: Boolean?, hotspotOn: Boolean?, now: Long, wall: Long): RadioObservation? {
        if (wifiOn == null || hotspotOn == null) return null
        if (wifi != null && wifi != wifiOn) if (wifiOn) wifiOnAt = now else wifiOffAt = now
        if (hotspot != null && hotspot != hotspotOn) if (hotspotOn) hotspotOnAt = now else hotspotOffAt = now
        wifi = wifiOn; hotspot = hotspotOn
        if (!(wifiOn && hotspotOn)) bothSince = null
        val evidence = when {
            wifiOn && hotspotOn -> {
                val since = bothSince ?: now.also { bothSince = it }
                if (now - since >= window) "both_on" else null
            }
            hotspotOn && near(wifiOffAt, hotspotOnAt) -> "wifi_off_with_hotspot"
            wifiOn && !hotspotOn && returned(hotspotOffAt, wifiOnAt) -> "wifi_back_after_hotspot"
            else -> null
        } ?: return null
        if (evidence != "both_on") { wifiOffAt = null; wifiOnAt = null; hotspotOnAt = null; hotspotOffAt = null }
        val exclusive = evidence != "both_on"
        if (observation?.exclusive == exclusive) return null
        return RadioObservation(exclusive, wall, evidence).also { observation = it; save(it) }
    }

    private fun near(first: Long?, second: Long?) = first != null && second != null && kotlin.math.abs(first - second) <= window
    private fun returned(off: Long?, on: Long?) = off != null && on != null && on >= off && on - off <= window
}

/** Observations belong to one system build: an update may change how the radios share the chip. */
class RadioObservationStore(context: Context, private val build: String = Build.FINGERPRINT.orEmpty()) {
    private val preferences = context.applicationContext.getSharedPreferences("radio-concurrency", Context.MODE_PRIVATE)
    fun read(): RadioObservation? = runCatching {
        if (preferences.getString("build", null) != build || !preferences.contains("exclusive")) return null
        RadioObservation(preferences.getBoolean("exclusive", false), preferences.getLong("at", 0),
            preferences.getString("evidence", null).orEmpty())
    }.getOrNull()
    fun write(value: RadioObservation) {
        runCatching { preferences.edit().putString("build", build).putBoolean("exclusive", value.exclusive)
            .putLong("at", value.atMillis).putString("evidence", value.evidence).apply() }
    }
}

/** Android 11+ reports whether the Wi-Fi chip runs a station next to the hotspot; older releases cannot tell. */
fun platformStaApConcurrency(context: Context): Boolean? = if (Build.VERSION.SDK_INT < 30) null else runCatching {
    context.applicationContext.getSystemService(WifiManager::class.java)?.isStaApConcurrencySupported
}.getOrNull()
