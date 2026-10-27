// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-27
package org.openxplay.direlay.admin

import org.json.JSONObject
import org.openxplay.direlay.path.*

/** The shared admin page consumes the same plain facts as Android's path presenter. */
internal fun PathInput.json(): JSONObject = obj(
    "platform" to "android",
    "nowMillis" to nowMillis,
    "version" to version,
    "setup" to setup.json(),
    "relay" to relay.json(),
    "car" to car.json(),
    "iphone" to iphone.json(),
    "live" to live.json(),
)

private fun SetupInput.json() = obj(
    "network" to network.name,
    "outputs" to outputs.map { it.name },
    "summary" to summary,
    "valid" to valid,
)

private fun RelayInput.json() = obj(
    "identity" to identity.name,
    "identityLabel" to identityLabel,
    "permissionsGranted" to permissionsGranted,
    "permissionsTotal" to permissionsTotal,
    "vpnConsentNeeded" to vpnConsentNeeded,
    "keepAlive" to keepAlive.name,
    "bluetoothOn" to bluetoothOn,
    "bluetoothName" to bluetoothName,
    "wifiOn" to wifiOn,
    "wifiSsid" to wifiSsid,
    "hotspotOn" to hotspotOn,
    "hotspotSsid" to hotspotSsid,
    "hotspotBand" to hotspotBand,
    "sharedWifiSsid" to sharedWifiSsid,
    "overlayNeeded" to overlayNeeded,
)

private fun CarInput.json() = obj(
    "address" to address,
    "web" to web.json(),
    "pageUrl" to pageUrl,
    "pages" to pages,
    "screen" to screen,
    "savedScreen" to savedScreen,
    "carlink" to carlink.json(),
    "dlna" to dlna.json(),
    "miracast" to miracast.json(),
    "tesla" to tesla.json(),
)

private fun WebServerInput.json() = obj(
    "phase" to phase.name,
    "port" to port,
    "code" to code,
    "trustedName" to trustedName,
    "sinceMillis" to sinceMillis,
)

private fun CarlinkInput.json() = obj(
    "phase" to phase.name,
    "sinceMillis" to sinceMillis,
    "code" to code,
    "error" to error,
    "retrying" to retrying,
    "carName" to carName,
    "joinMethod" to joinMethod,
    "trusted" to trusted,
    "pinPending" to pinPending,
    "picture" to picture,
    "trail" to trail,
)

private fun OutputInput.json() = obj(
    "phase" to phase.name,
    "name" to name,
    "code" to code,
    "sinceMillis" to sinceMillis,
)

private fun IphoneInput.json() = obj(
    "pick" to pick.name,
    "name" to name,
    "usb" to usb.name,
    "started" to started,
    "armed" to armed,
    "carWaitUntilMillis" to carWaitUntilMillis,
    "step" to step.name,
    "stepSinceMillis" to stepSinceMillis,
    "stepDetail" to stepDetail,
    "failure" to failure?.json(),
)

private fun LinkFailure.json() = obj(
    "step" to step.name,
    "code" to code,
    "text" to text,
    "atMillis" to atMillis,
    "retrying" to retrying,
    "trail" to trail,
)

private fun LiveInput.json() = obj(
    "pictureOnCar" to pictureOnCar,
    "sessionSinceMillis" to sessionSinceMillis,
    "chips" to chips,
)
