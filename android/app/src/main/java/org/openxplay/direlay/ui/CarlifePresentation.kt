// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-11-05
package org.openxplay.direlay.ui

import androidx.compose.foundation.layout.*
import androidx.compose.foundation.selection.selectableGroup
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import org.openxplay.direlay.R
import org.openxplay.direlay.settings.*
import org.openxplay.direlay.ui.design.*

fun carlifeOption(transport: CarlifeTransport) = when (transport) {
    CarlifeTransport.AUTO -> RadioOption(R.string.carlife_auto, R.string.carlife_auto_hint)
    CarlifeTransport.USB -> RadioOption(R.string.carlife_usb, R.string.carlife_usb_hint)
    CarlifeTransport.WIFI -> RadioOption(R.string.carlife_wifi, R.string.carlife_wifi_hint)
}
fun carlifeSummary(transport: CarlifeTransport) = when (transport) {
    CarlifeTransport.AUTO -> R.string.ui_carlife
    CarlifeTransport.USB -> R.string.carlife_usb_summary
    CarlifeTransport.WIFI -> R.string.carlife_wifi_summary
}
fun carlifeNetworkOption(settings: RelaySettings, choice: WirelessNetworkChoice): RadioOption {
    if (!CarlifeConnectionPolicy.usbOutput(settings)) return networkOption(choice)
    return when (choice) {
        WirelessNetworkChoice.WIFI_DIRECT -> RadioOption(R.string.direlay_wifi_direct, R.string.carlife_usb_direct_hint)
        WirelessNetworkChoice.PHONE_HOTSPOT -> RadioOption(choice.label, R.string.carlife_usb_hotspot_hint)
        WirelessNetworkChoice.EXISTING_WIFI -> RadioOption(R.string.carlife_shared_wifi, R.string.carlife_usb_shared_hint)
    }
}

@Composable fun CarlifeTransportOptions(settings: RelaySettings, includeAutomatic: Boolean = false, change: (RelaySettings) -> Unit) {
    Text(stringResource(R.string.carlife_transport_title), style = RelayTokens.headline)
    Column(Modifier.selectableGroup()) {
        CarlifeTransport.entries.filter { includeAutomatic || it != CarlifeTransport.AUTO }.forEach { transport ->
            RadioOptionRow(carlifeOption(transport), settings.carlifeTransport == transport) {
                change(CarlifeConnectionPolicy.choose(settings, transport))
            }
        }
    }
}

/** Both link captions refer to actual transports; the USB car link never depends on an iPhone network choice. */
data class CarlifeRoute(val iphone: Int, val car: Int)
fun carlifeRoute(settings: RelaySettings, wireless: Boolean) = CarlifeRoute(
    if (!wireless) R.string.carlife_usb_link else when (settings.wirelessNetwork.choice) {
        WirelessNetworkChoice.WIFI_DIRECT -> R.string.network_direct
        WirelessNetworkChoice.PHONE_HOTSPOT -> R.string.network_hotspot
        WirelessNetworkChoice.EXISTING_WIFI -> R.string.network_wifi
    }, when (settings.carlifeTransport) {
        CarlifeTransport.AUTO -> R.string.carlife_auto_link
        CarlifeTransport.USB -> R.string.carlife_usb_link
        CarlifeTransport.WIFI -> R.string.network_wifi
    })

@Composable fun CarlifeRouteDiagram(settings: RelaySettings, wireless: Boolean) {
    val route = carlifeRoute(settings, wireless)
    val description = stringResource(R.string.carlife_route_description, stringResource(route.iphone), stringResource(route.car))
    Column(Modifier.fillMaxWidth().clearAndSetSemantics { contentDescription = description },
        verticalArrangement = Arrangement.spacedBy(12.dp)) {
        Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(4.dp), verticalAlignment = Alignment.Top) {
            RouteNode(R.drawable.symbol_smartphone, R.string.network_iphone, IconHue.IPHONE, Modifier.weight(1f))
            RouteLink(route.iphone, wireless, Modifier.weight(1f))
            RouteNode(R.drawable.symbol_smartphone, R.string.carlife_android_phone, IconHue.NETWORK, Modifier.weight(1.2f))
            RouteLink(route.car, when (settings.carlifeTransport) {
                CarlifeTransport.AUTO -> null; CarlifeTransport.USB -> false; CarlifeTransport.WIFI -> true
            }, Modifier.weight(1f))
            RouteNode(R.drawable.symbol_car_front, R.string.network_car, IconHue.CAR, Modifier.weight(1f))
        }
        Text(description, style = RelayTokens.footnote, color = MaterialTheme.colorScheme.onSurfaceVariant)
    }
}

@Composable private fun RouteNode(icon: Int, caption: Int, hue: IconHue, modifier: Modifier) {
    Column(modifier, horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(8.dp)) {
        IconTile(icon, hue, Modifier.size(40.dp))
        Text(stringResource(caption), style = RelayTokens.caption, textAlign = TextAlign.Center)
    }
}

@Composable private fun RouteLink(caption: Int, wireless: Boolean?, modifier: Modifier) {
    Column(modifier, horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(4.dp)) {
        Row(Modifier.height(40.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(2.dp)) {
            Symbol(when (wireless) { true -> R.drawable.symbol_wifi; false -> R.drawable.symbol_usb; null -> R.drawable.symbol_link }, null, Modifier.size(24.dp))
            Symbol(R.drawable.symbol_chevron, null, Modifier.size(12.dp))
        }
        Text(stringResource(caption), style = RelayTokens.caption, textAlign = TextAlign.Center)
    }
}
