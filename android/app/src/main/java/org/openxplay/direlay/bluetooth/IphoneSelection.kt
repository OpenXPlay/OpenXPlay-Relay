// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-15
package org.openxplay.direlay.bluetooth

data class BondedDevice(val address: String, val name: String?, val phoneClass: Boolean,
    val iap2: Boolean, val connected: Boolean, val carClass: Boolean = false) {
    val strong: Boolean get() = iap2 || name.orEmpty().contains("iPhone", ignoreCase = true)
}

sealed class IphoneSelection(val address: String? = null) {
    data class Selected(val device: BondedDevice) : IphoneSelection(device.address)
    data class SelectedUnpaired(val chosenAddress: String) : IphoneSelection()
    data class Connected(val device: BondedDevice) : IphoneSelection(device.address)
    data class MultipleConnected(val devices: List<BondedDevice>) : IphoneSelection()
    data class OnlyCandidate(val device: BondedDevice) : IphoneSelection(device.address)
    data class MultipleCandidates(val devices: List<BondedDevice>) : IphoneSelection()
    data class OnlyBonded(val device: BondedDevice) : IphoneSelection(device.address)
    data object NoIphone : IphoneSelection()

    // Stable reason codes remain readable after release-build class obfuscation.
    val reason: String get() = when (this) {
        is Selected -> "Selected"
        is SelectedUnpaired -> "SelectedUnpaired"
        is Connected -> "Connected"
        is MultipleConnected -> "MultipleConnected"
        is OnlyCandidate -> "OnlyCandidate"
        is MultipleCandidates -> "MultipleCandidates"
        is OnlyBonded -> "OnlyBonded"
        NoIphone -> "NoIphone"
    }

    /** Diagnostics deliberately expose only the last two address octets. */
    fun diagnostic(): String {
        val addresses = when (this) {
            is SelectedUnpaired -> listOf(chosenAddress)
            is MultipleConnected -> devices.map { it.address }
            is MultipleCandidates -> devices.map { it.address }
            else -> listOfNotNull(address)
        }
        return reason + addresses.takeIf { it.isNotEmpty() }
            ?.joinToString(prefix = " · ") { maskedAddress(it) }.orEmpty()
    }

    companion object {
        fun select(choice: String?, bonded: List<BondedDevice>): IphoneSelection {
            if (choice != null) return bonded.firstOrNull { it.address.equals(choice, ignoreCase = true) }
                ?.let(::Selected) ?: SelectedUnpaired(choice)
            val strong = bonded.filter { it.strong }
            val candidates = strong.ifEmpty { bonded.filter { it.phoneClass } }
            val connected = candidates.filter { it.connected }
            return when {
                connected.size == 1 -> Connected(connected.single())
                connected.size > 1 -> MultipleConnected(connected)
                candidates.size == 1 -> OnlyCandidate(candidates.single())
                candidates.size > 1 -> MultipleCandidates(candidates)
                bonded.size == 1 -> OnlyBonded(bonded.single())
                else -> NoIphone
            }
        }
    }
}

fun maskedAddress(address: String): String =
    address.split(':').takeIf { it.size == 6 && it.all { part -> part.matches(Regex("[0-9a-fA-F]{2}")) } }
        ?.takeLast(2)?.joinToString(":", prefix = "…:") ?: "—"
