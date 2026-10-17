// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-15
package org.openxplay.direlay.admin

import android.content.Context
import org.openxplay.direlay.bluetooth.IphoneSelection
import org.openxplay.direlay.relay.RelayRuntime
import org.openxplay.direlay.relay.receiverUrl
import org.openxplay.direlay.ui.SettingsNotice
import org.openxplay.direlay.ui.settingsNotice
import org.openxplay.stream.core.VideoCodecDecision

fun interface AdminActions { fun action(name: String): AdminResult }
/** The codec in use and why, plus the iPhone stream's codec string and size once known. */
internal fun videoStatus(current: String, decision: VideoCodecDecision, codecString: String?, width: Int?, height: Int?): Map<String, Any?> =
    mapOf("codec" to current, "reason" to if (current == decision.codec.name) decision.reason else "${decision.reason}; codec-change-pending",
        "codecString" to codecString, "width" to width, "height" to height)
class AndroidRelayAdmin(private val context: Context, private val runtime: RelayRuntime,
    private val actions: AdminActions, private val pairing: PairingWindow = runtime.pairing,
    private val advertisedHost: () -> String?) : RelayAdmin {
    private val pendingRestart = ThreadLocal<Boolean>()
    override fun responseSent() {
        val restart = pendingRestart.get() == true
        pendingRestart.remove()
        if (restart) actions.action("restart")
    }
    override fun responseFailed() { pendingRestart.remove() }
    override fun status(): AdminStatus {
        val settings = runtime.settingsStore.awaitSnapshot()
        runtime.refreshIphoneBonds()
        val selected = runtime.iphoneSelection()
        val device = runtime.iphoneBonds.firstOrNull { it.address == selected.address }
        val reason = when (selected) {
            is IphoneSelection.Selected -> "selected"; is IphoneSelection.SelectedUnpaired -> "unpaired"
            is IphoneSelection.Connected -> "connected"; is IphoneSelection.OnlyCandidate, is IphoneSelection.OnlyBonded -> "onlyCandidate"
            is IphoneSelection.MultipleCandidates, is IphoneSelection.MultipleConnected -> "multiple"; else -> "none"
        }
        val host = advertisedHost() ?: "openxplay-android.local"
        val running = runtime.autoStartSessions.tracker.running
        val phase = runtime.status.session?.phase?.name
        val state = when {
            runtime.startRequest.pending -> "connecting"
            !running -> "idle"
            phase == "ACTIVE" -> "active"
            phase == "RECONNECTING" -> "reconnecting"
            else -> "waiting"
        }
        val wifi = org.openxplay.direlay.settings.currentWifiSsid(context)
        val now = System.currentTimeMillis() / 1000
        val window = pairing.state(now)
        val format = runtime.status.videoFormat
        val port = settings.webPort ?: 8443
        val receivers = if (!settings.carlinkOutput || settings.bothOutputs)
            (listOf(host) + runtime.carIpv4.sorted()).map {
                receiverUrl("https://$it:$port/", settings.requirePairingCode, runtime.pairingCode)
            } else emptyList()
        return AdminStatus("android", "OpenXPlay Relay", context.packageManager.getPackageInfo(context.packageName, 0).versionName.orEmpty(),
            host, "https://$host:6975/", receivers, state, running, runtime.autoStartGate.paused,
            AdminIphone(device?.name, reason), AdminNetwork(AdminSettingsMapping.network(settings), wifi),
            AdminSettingsMapping.output(settings), if (settings.carlinkOutput || settings.bothOutputs) AdminCarlink(runtime.carlinkState, runtime.carlinkDetail) else null,
            if (runtime.autoStartGate.paused) listOf("notice.autoStartPaused") else emptyList(),
            video = videoStatus((runtime.status.videoCodec ?: runtime.appliedVideoDecision.codec).name, runtime.videoDecision,
                format?.codecString, format?.width, format?.height),
            pairingAvailable = pairing.available, pairingDeadline = window.deadline, pairingRemaining = window.deadline?.let { it - now },
            pairingNeedsConfirmation = window.needsConfirmation, pairingOnRelay = window.onRelay,
            pairingNeedsPermission = window.needsPermission, pairingPermissionDenied = window.permissionDenied,
            pairingNotified = window.notified,
            pairedDevices = runtime.iphoneBonds.map { AdminPairedDevice(it.name.orEmpty(), it.connected, it.strong) },
            hostAddresses = try {
                java.net.NetworkInterface.getNetworkInterfaces()?.toList().orEmpty().filter { it.isUp && !it.isLoopback }.flatMap { network ->
                    network.inetAddresses.toList().filterIsInstance<java.net.Inet4Address>().filter { !it.isLoopbackAddress }.map { address ->
                        AdminHostAddress(network.name, address.hostAddress.orEmpty(), listOf("usb", "rndis", "veth", "docker", "tun", "wg").any { network.name.startsWith(it) })
                    }
                }
            } catch (_: java.net.SocketException) { emptyList() }, receiverPort = port,
            hotspot = settings.wirelessNetwork.takeIf {
                it.choice == org.openxplay.direlay.settings.WirelessNetworkChoice.PHONE_HOTSPOT && it.manualSsid.isNotEmpty()
            }?.let { AdminHotspot(it.manualSsid, it.manualPassphrase, if (running) runtime.carIpv4.sorted().firstOrNull() else null, running) })
    }
    override fun settings(): AdminSettings {
        val value = runtime.settingsStore.awaitSnapshot()
        return AdminSettings(AdminSettingsMapping.schema, AdminSettingsMapping.values(value, runtime.settingsStore.wireless(value), org.openxplay.direlay.settings.currentWifiSsid(context)),
            runtime.installedPlugins())
    }
    override fun update(values: Map<String, JsonElement>): AdminUpdate {
        val store = runtime.settingsStore
        store.awaitSnapshot()
        return synchronized(store) { updateLoaded(values) }
    }
    private fun updateLoaded(values: Map<String, JsonElement>): AdminUpdate {
        val store = runtime.settingsStore
        val before = checkNotNull(store.snapshot.value)
        val mapped = AdminSettingsMapping.update(before, values, store.wireless(before))
        if (mapped.errors.isNotEmpty()) return AdminUpdate(errors = mapped.errors)
        val after = checkNotNull(mapped.settings)
        if (values.containsKey("pluginsEnabled")) {
            val installed = runtime.installedPlugins().map { it.id }.toSet()
            if (after.pluginsEnabled.any { it !in installed }) return AdminUpdate(errors = mapOf("pluginsEnabled" to "error.plugins"))
        }
        val notice = settingsNotice(before, after, runtime.carlinkConnected)
        return try {
            store.save(after); runtime.reloadSettings()
            val modeChanged = before.wirelessNetwork.choice != after.wirelessNetwork.choice
            if (modeChanged && runtime.autoStartSessions.tracker.running) pendingRestart.set(true)
            AdminUpdate(if (modeChanged || notice == SettingsNotice.SILENT) "now" else "next-start")
        } catch (_: IllegalArgumentException) { AdminUpdate(errors = values.keys.associateWith { "error.invalid" }) }
    }
    override fun action(name: String): AdminResult = when (name) {
        "regenerate-pairing-code" -> { runtime.regeneratePairingCode(); AdminResult() }
        "pair-iphone" -> pairing.open()
        // Android has no public API to end Bluetooth visibility early.
        "cancel-pairing" -> AdminResult("error.action")
        else -> actions.action(name)
    }
    override fun receivers(): List<AdminReceiver> =
        runtime.status.receivers.snapshot().map { AdminReceiver(it.client, it.idleSeconds, it.info, it.stats) }
    override fun phones(): List<AdminPhone> {
        runtime.refreshIphoneBonds()
        val selected = runtime.settingsStore.awaitSnapshot().iphoneAddress
        return runtime.iphoneBonds.map { AdminPhone(it.address, it.name ?: it.address,
            if (it.carClass) "car" else if (it.phoneClass || it.strong) "phone" else "other", it.connected, it.strong,
            it.address.equals(selected, ignoreCase = true)) }
    }
}
