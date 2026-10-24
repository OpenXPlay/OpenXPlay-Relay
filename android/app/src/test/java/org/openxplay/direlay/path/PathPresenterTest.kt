// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-24
package org.openxplay.direlay.path

import java.time.Instant
import java.time.ZoneId
import org.junit.Assert.*
import org.junit.Test
import org.openxplay.direlay.R
import org.openxplay.direlay.path.StepAction.*
import org.openxplay.direlay.path.StepId.*
import org.openxplay.direlay.path.StepState.*

class PathPresenterTest {
    private val utc = ZoneId.of("UTC")
    private val now = Instant.parse("2026-10-24T14:32:05Z").toEpochMilli()
    private val ready = PathInput(nowMillis = now, version = "0.1.6",
        setup = SetupInput(NetworkPlan.PHONE_HOTSPOT, setOf(CarOutput.BROWSER), "Wireless · Hotspot · Browser"),
        relay = RelayInput(identity = IdentityState.READY, identityLabel = "Installed",
            permissionsGranted = 4, permissionsTotal = 4, bluetoothOn = true, bluetoothName = "Relay",
            wifiOn = true, wifiSsid = "Home", hotspotOn = true, hotspotSsid = "OpenXPlay", hotspotBand = "5 GHz",
            sharedWifiSsid = "Home"),
        car = CarInput(address = "192.168.43.1", web = WebServerInput(WebPhase.LISTENING, 8443),
            pageUrl = "https://192.168.43.1:8443", pages = 1, screen = "1920×720"),
        iphone = IphoneInput(pick = IphonePick.PICKED, name = "iPhone 15", usb = UsbRole.HOST))

    private fun path(input: PathInput = ready) = presentPath(input, utc)
    private fun PathInput.step(id: StepId) = path(this).steps.single { it.id == id }
    private fun phone(step: LinkStep, input: PathInput = ready) = input.copy(iphone = input.iphone.copy(
        started = true, step = step, stepSinceMillis = now - 5000))
    private fun carlink(link: CarlinkInput, input: PathInput = ready) = input.copy(
        setup = input.setup.copy(outputs = setOf(CarOutput.CARLINK)), car = input.car.copy(carlink = link))
    private fun output(id: CarOutput, value: OutputInput, input: PathInput = ready) = input.copy(
        setup = input.setup.copy(outputs = setOf(id)), car = when (id) {
            CarOutput.DLNA -> input.car.copy(dlna = value)
            CarOutput.MIRACAST -> input.car.copy(miracast = value)
            CarOutput.TESLA -> input.car.copy(tesla = value)
            else -> input.car
        })
    private fun failed(code: String, step: LinkStep, retrying: Boolean = false, input: PathInput = ready) =
        phone(step, input).let { it.copy(iphone = it.iphone.copy(failure = LinkFailure(
            step, code, "Failure: $code", now - 7000, retrying, listOf("first", "second")))) }

    @Test fun setupStepListsAreExactAndSectionsStayInOrder() {
        val common = listOf(MODE, IDENTITY, PERMISSIONS)
        val browser = listOf(CAR_NETWORK, WEB_SERVER, CAR_PAGE)
        val clink = listOf(CARLINK_FIND, CARLINK_LINK, CARLINK_PIN, CARLINK_READY)
        val wireless = listOf(IPHONE_PAIRED, CARPLAY_START, IPHONE_BLUETOOTH, IPHONE_AUTH, IPHONE_WIFI, SESSION)
        val wired = listOf(USB_HOST, CARPLAY_START, USB_IPHONE, USB_TRUST, USB_CONTROL, SESSION)
        val cases = listOf(
            Triple(NetworkPlan.PHONE_HOTSPOT, setOf(CarOutput.BROWSER), common + listOf(BLUETOOTH, HOTSPOT) + browser + wireless + PICTURE),
            Triple(NetworkPlan.PHONE_HOTSPOT, setOf(CarOutput.CARLINK), common + listOf(BLUETOOTH, WIFI, HOTSPOT) + clink + wireless + PICTURE),
            Triple(NetworkPlan.PHONE_HOTSPOT, setOf(CarOutput.CARLINK, CarOutput.BROWSER), common + listOf(BLUETOOTH, WIFI, HOTSPOT) + browser + clink + wireless + PICTURE),
            Triple(NetworkPlan.WIFI_DIRECT, setOf(CarOutput.BROWSER), common + listOf(BLUETOOTH, WIFI) + browser + wireless + PICTURE),
            Triple(NetworkPlan.SHARED_WIFI, setOf(CarOutput.DLNA), common + listOf(BLUETOOTH, WIFI, SHARED_WIFI) + DLNA + wireless + PICTURE),
            Triple(NetworkPlan.USB, setOf(CarOutput.BROWSER), common + browser + wired + PICTURE),
            Triple(NetworkPlan.PHONE_HOTSPOT, setOf(CarOutput.TESLA), common + listOf(BLUETOOTH, HOTSPOT) + wireless + listOf(TESLA, PICTURE)),
            Triple(NetworkPlan.WIFI_DIRECT, setOf(CarOutput.MIRACAST), common + listOf(BLUETOOTH, WIFI) + MIRACAST + wireless + PICTURE),
            Triple(NetworkPlan.USB, emptySet<CarOutput>(), common + wired + PICTURE),
        )
        cases.forEach { (network, outputs, expected) ->
            val actual = path(ready.copy(setup = ready.setup.copy(network = network, outputs = outputs)))
            assertEquals("$network $outputs", expected, actual.steps.map { it.id })
            assertEquals(actual.sections.map { it.section }.sortedBy { it.order }, actual.sections.map { it.section })
            assertTrue(actual.sections.all { it.steps.isNotEmpty() && it.steps.all { step -> step.id.section == it.section } })
        }
    }

    @Test fun everySetupCombinationContainsOnlyItsOwnSteps() {
        NetworkPlan.entries.forEach { network ->
            repeat(1 shl CarOutput.entries.size) { mask ->
                val outputs = CarOutput.entries.filterIndexed { index, _ -> mask and (1 shl index) != 0 }.toSet()
                val actual = path(ready.copy(setup = ready.setup.copy(network = network, outputs = outputs)))
                val steps = actual.steps.map { it.id }
                assertEquals(PathSection.entries.filter { section -> steps.any { it.section == section } }, actual.sections.map { it.section })
                assertTrue(actual.sections.all { it.steps.isNotEmpty() })
                assertEquals(steps.distinct(), steps)
                assertEquals(CarOutput.BROWSER in outputs, CAR_NETWORK in steps)
                assertEquals(CarOutput.BROWSER in outputs, WEB_SERVER in steps)
                assertEquals(CarOutput.BROWSER in outputs, CAR_PAGE in steps)
                listOf(CARLINK_FIND, CARLINK_LINK, CARLINK_PIN, CARLINK_READY).forEach {
                    assertEquals(CarOutput.CARLINK in outputs, it in steps)
                }
                assertEquals(CarOutput.DLNA in outputs, DLNA in steps)
                assertEquals(CarOutput.MIRACAST in outputs, MIRACAST in steps)
                assertEquals(CarOutput.TESLA in outputs, TESLA in steps)
                assertEquals(network != NetworkPlan.USB || CarOutput.CARLINK in outputs, BLUETOOTH in steps)
                assertEquals(network in setOf(NetworkPlan.WIFI_DIRECT, NetworkPlan.SHARED_WIFI) ||
                    outputs.any { it in setOf(CarOutput.CARLINK, CarOutput.MIRACAST) }, WIFI in steps)
                assertEquals(network == NetworkPlan.PHONE_HOTSPOT, HOTSPOT in steps)
                assertEquals(network == NetworkPlan.SHARED_WIFI, SHARED_WIFI in steps)
                listOf(IPHONE_PAIRED, IPHONE_BLUETOOTH, IPHONE_AUTH, IPHONE_WIFI).forEach {
                    assertEquals(network != NetworkPlan.USB, it in steps)
                }
                listOf(USB_HOST, USB_IPHONE, USB_TRUST, USB_CONTROL).forEach {
                    assertEquals(network == NetworkPlan.USB, it in steps)
                }
                assertEquals(listOf(MODE, IDENTITY, PERMISSIONS), steps.take(3))
                assertEquals(PICTURE, steps.last())
            }
        }
    }

    @Test fun relayStepsUseExactDetailsEvidenceAndActions() {
        assertEquals(StepText.raw(ready.setup.summary), ready.step(MODE).evidence)
        assertEquals(StepText.raw("Installed"), ready.step(IDENTITY).evidence)
        assertEquals(StepText.raw("4/4"), ready.step(PERMISSIONS).evidence)
        assertEquals(StepText.raw("Relay"), ready.step(BLUETOOTH).evidence)
        assertEquals(StepText.raw("OpenXPlay · 5 GHz"), ready.step(HOTSPOT).evidence)
        val missing = ready.copy(setup = ready.setup.copy(valid = false)).step(MODE)
        assertEquals(ACTION, missing.state)
        assertEquals(StepText.of(R.string.path_settings_missing), missing.detail)
        assertEquals(FIX_SETTINGS, missing.action)
        assertEquals(R.string.path_action_fix_settings, missing.actionLabel)
        assertEquals(WORKING, ready.copy(relay = ready.relay.copy(identity = IdentityState.CHECKING)).step(IDENTITY).state)
        assertEquals(IMPORT_IDENTITY, ready.copy(relay = ready.relay.copy(identity = IdentityState.MISSING)).step(IDENTITY).action)
        val permissions = ready.copy(relay = ready.relay.copy(permissionsGranted = 2)).step(PERMISSIONS)
        assertEquals(ACTION, permissions.state)
        assertEquals(StepText.of(R.string.path_permissions_needed, "2"), permissions.detail)
        assertEquals(GRANT_PERMISSIONS, permissions.action)
        assertEquals(StepText.of(R.string.path_allow_vpn), ready.copy(relay = ready.relay.copy(
            permissionsGranted = 3, vpnConsentNeeded = true)).step(PERMISSIONS).detail)
        assertEquals(ACTION, ready.copy(relay = ready.relay.copy(vpnConsentNeeded = true)).step(PERMISSIONS).state)
        KeepAliveState.entries.forEach { state ->
            val value = ready.copy(relay = ready.relay.copy(keepAlive = state))
            if (state in setOf(KeepAliveState.NOT_NEEDED, KeepAliveState.FOREGROUND)) assertFalse(path(value).steps.any { it.id == KEEP_ALIVE })
            else {
                assertEquals(if (state == KeepAliveState.PENDING) WORKING else ACTION, value.step(KEEP_ALIVE).state)
                assertEquals(if (state == KeepAliveState.REFUSED) KEEP_ALIVE_SETTINGS else null, value.step(KEEP_ALIVE).action)
                assertEquals(WORKING, phone(LinkStep.STARTING, value).step(CARPLAY_START).state)
                assertEquals(BLOCKED, value.step(CARPLAY_START).state)
            }
        }
    }

    @Test fun radiosHaveOnOffAndUnreadableStatesAndSharedWifiMustBeSaved() {
        val shared = ready.copy(setup = ready.setup.copy(network = NetworkPlan.SHARED_WIFI))
        assertEquals(StepText.raw("Home"), shared.step(WIFI).evidence)
        assertEquals(StepText.raw("Home"), shared.step(SHARED_WIFI).evidence)
        assertEquals(CHOOSE_SHARED_WIFI, shared.copy(relay = shared.relay.copy(sharedWifiSsid = null)).step(SHARED_WIFI).action)
        assertEquals(StepText.of(R.string.path_on), shared.copy(relay = shared.relay.copy(wifiSsid = null)).step(WIFI).evidence)
        assertEquals(StepText.of(R.string.path_on), ready.copy(relay = ready.relay.copy(bluetoothName = null)).step(BLUETOOTH).evidence)
        val off = shared.copy(relay = shared.relay.copy(wifiOn = false))
        assertEquals(ACTION, off.step(WIFI).state)
        assertEquals(OPEN_WIFI, off.step(WIFI).action)
        assertEquals(TURN_ON_BLUETOOTH, ready.copy(relay = ready.relay.copy(bluetoothOn = false)).step(BLUETOOTH).action)
        assertEquals(OPEN_HOTSPOT, ready.copy(relay = ready.relay.copy(hotspotOn = false)).step(HOTSPOT).action)
        val unreadable = phone(LinkStep.AUTH, ready.copy(relay = ready.relay.copy(bluetoothOn = null, hotspotOn = null)))
        assertEquals(WAITING, unreadable.step(BLUETOOTH).state)
        assertEquals(WAITING, unreadable.step(HOTSPOT).state)
        assertEquals(OK, unreadable.step(IPHONE_PAIRED).state)
        assertEquals(OK, unreadable.step(CARPLAY_START).state)
        assertEquals(WORKING, unreadable.step(IPHONE_AUTH).state)
        val p2p = phone(LinkStep.STARTING, ready.copy(setup = ready.setup.copy(network = NetworkPlan.WIFI_DIRECT),
            relay = ready.relay.copy(wifiOn = null)))
        assertEquals(WAITING, p2p.step(WIFI).state)
        assertEquals(WORKING, p2p.step(CARPLAY_START).state)
    }

    @Test fun prerequisitesBlockTransitivelyAndKeepSpecificDetails() {
        // Proved steps keep OK; only steps that still wait take BLOCKED from a failing prerequisite.
        val invalid = ready.copy(setup = ready.setup.copy(valid = false), car = ready.car.copy(pages = 0),
            relay = ready.relay.copy(identity = IdentityState.MISSING))
        assertEquals(BLOCKED, invalid.step(IDENTITY).state)
        assertEquals(StepText.of(R.string.path_import_identity), invalid.step(IDENTITY).detail)
        assertEquals(OK, invalid.step(PERMISSIONS).state)
        assertEquals(OK, invalid.step(WEB_SERVER).state)
        assertEquals(BLOCKED, invalid.step(CARPLAY_START).state)
        val unpairedOff = ready.copy(relay = ready.relay.copy(bluetoothOn = false),
            iphone = ready.iphone.copy(pick = IphonePick.NONE))
        assertEquals(BLOCKED, unpairedOff.step(IPHONE_PAIRED).state)
        val hotspotOff = ready.copy(relay = ready.relay.copy(hotspotOn = false), car = ready.car.copy(address = null, pages = 0))
        assertEquals(BLOCKED, hotspotOff.step(CAR_NETWORK).state)
        assertEquals(StepText.of(R.string.path_tap_connect), hotspotOff.step(CARPLAY_START).detail)
        assertEquals(BLOCKED, hotspotOff.step(CAR_PAGE).state)
        assertEquals(COPY_PAGE_ADDRESS, hotspotOff.step(CAR_PAGE).action)
        val noNetwork = ready.copy(car = ready.car.copy(address = null, pages = 0))
        assertEquals(WAITING, noNetwork.step(CAR_NETWORK).state)
        assertEquals(BLOCKED, noNetwork.step(CAR_PAGE).state)
        val noServer = ready.copy(car = ready.car.copy(pages = 0, web = ready.car.web.copy(phase = WebPhase.STARTING)))
        assertEquals(BLOCKED, noServer.step(CAR_PAGE).state)
        val shared = output(CarOutput.DLNA, OutputInput(OutputPhase.READY), ready.copy(
            setup = ready.setup.copy(network = NetworkPlan.SHARED_WIFI), relay = ready.relay.copy(sharedWifiSsid = null)))
        // With no browser network step, DLNA depends on Wi-Fi rather than the saved-network selection.
        assertEquals(OK, shared.step(DLNA).state)
        val withBrowser = shared.copy(setup = shared.setup.copy(outputs = setOf(CarOutput.DLNA, CarOutput.BROWSER)))
        assertEquals(OK, withBrowser.step(DLNA).state)
        val choose = withBrowser.copy(car = withBrowser.car.copy(dlna = OutputInput(OutputPhase.WAITING_USER), address = null))
        assertEquals(BLOCKED, choose.step(DLNA).state)
        val wifiOff = output(CarOutput.MIRACAST, OutputInput(OutputPhase.PLAYING), ready.copy(relay = ready.relay.copy(wifiOn = false)))
        assertEquals(OK, wifiOff.step(MIRACAST).state)
        assertEquals(BLOCKED, output(CarOutput.MIRACAST, OutputInput(OutputPhase.WAITING_USER), wifiOff).step(MIRACAST).state)
        assertEquals(OK, carlink(CarlinkInput(CarlinkPhase.PLAYING), wifiOff).step(CARLINK_FIND).state)
        assertEquals(BLOCKED, carlink(CarlinkInput(CarlinkPhase.SCANNING), wifiOff).step(CARLINK_FIND).state)
        val denied = ready.copy(relay = ready.relay.copy(permissionsGranted = 0, bluetoothOn = false, hotspotOn = false))
        assertEquals(BLOCKED, denied.step(BLUETOOTH).state)
        assertEquals(BLOCKED, denied.step(HOTSPOT).state)
    }

    @Test fun liveSessionKeepsProvedStepsWhenARadioDrops() {
        val live = phone(LinkStep.VIDEO, ready.copy(relay = ready.relay.copy(bluetoothOn = false),
            live = LiveInput(pictureOnCar = true, sessionSinceMillis = now - 60_000, chips = listOf("1920×720", "60 fps"))))
        assertEquals(ACTION, live.step(BLUETOOTH).state)
        listOf(IPHONE_PAIRED, CARPLAY_START, IPHONE_BLUETOOTH, IPHONE_AUTH, IPHONE_WIFI, SESSION, PICTURE).forEach {
            assertEquals(it.name, OK, live.step(it).state)
        }
        assertEquals(PathTone.LIVE, path(live).tone)
    }

    @Test fun browserStepStatesAndTrustedEvidence() {
        assertEquals(StepText.raw(":8443 HTTPS"), ready.step(WEB_SERVER).evidence)
        assertEquals(StepText.raw("relay.example"), ready.copy(car = ready.car.copy(web = ready.car.web.copy(trustedName = "relay.example"))).step(WEB_SERVER).evidence)
        listOf(WebPhase.OFF, WebPhase.STARTING).forEach { phase ->
            assertEquals(WORKING, ready.copy(car = ready.car.copy(web = ready.car.web.copy(phase = phase))).step(WEB_SERVER).state)
        }
        val busy = ready.copy(car = ready.car.copy(web = ready.car.web.copy(phase = WebPhase.FAILED, code = "web_port_busy"))).step(WEB_SERVER)
        assertEquals(FAILED, busy.state)
        assertEquals(StepText.of(R.string.path_port_busy, "8443"), busy.detail)
        assertEquals(R.string.path_explain_web_port_busy, busy.explanation)
        val failure = ready.copy(car = ready.car.copy(web = ready.car.web.copy(phase = WebPhase.FAILED))).step(WEB_SERVER)
        assertEquals("web_failed", failure.code)
        assertEquals(StepText.of(R.string.path_could_not_start), failure.detail)
        val waiting = ready.copy(car = ready.car.copy(pages = 0)).step(CAR_PAGE)
        assertEquals(WAITING, waiting.state)
        assertEquals(COPY_PAGE_ADDRESS, waiting.action)
        assertEquals(StepText.raw(ready.car.pageUrl!!), waiting.evidence)
        assertEquals(OK, ready.step(CAR_PAGE).state)
        assertEquals(StepText.raw("1920×720"), ready.step(CAR_PAGE).evidence)
    }

    @Test fun canStartIgnoresRadiosAndCarButRequiresIdentityPermissionsAndPick() {
        assertTrue(path().canStart)
        assertEquals(PathTone.READY, path().tone)
        listOf(ready.copy(relay = ready.relay.copy(identity = IdentityState.MISSING)),
            ready.copy(relay = ready.relay.copy(permissionsGranted = 0)),
            ready.copy(relay = ready.relay.copy(vpnConsentNeeded = true)),
            ready.copy(setup = ready.setup.copy(valid = false)),
            ready.copy(iphone = ready.iphone.copy(pick = IphonePick.NONE))).forEach { assertFalse(path(it).canStart) }
        assertTrue(path(ready.copy(relay = ready.relay.copy(bluetoothOn = false, hotspotOn = false),
            car = CarInput())).canStart)
        val wired = ready.copy(setup = ready.setup.copy(network = NetworkPlan.USB),
            iphone = ready.iphone.copy(pick = IphonePick.NONE, usb = UsbRole.DISCONNECTED))
        assertTrue(path(wired).canStart)
        assertEquals(BLOCKED, wired.step(CARPLAY_START).state)
        assertEquals(StepText.of(R.string.path_tap_connect), wired.step(CARPLAY_START).detail)
        val armed = ready.copy(relay = ready.relay.copy(hotspotOn = false), iphone = ready.iphone.copy(armed = true))
        assertTrue(path(armed).running)
        assertFalse(path(armed).canStart)
        assertEquals(ACTION, armed.step(HOTSPOT).state)
        assertEquals(BLOCKED, armed.step(CARPLAY_START).state)
        assertEquals(StepText.of(R.string.path_start_armed), armed.step(CARPLAY_START).detail)
        assertEquals(PathTone.PROGRESS, path(armed.copy(relay = ready.relay)).tone)
    }

    @Test fun carWaitUsesCeilingSecondsAndNeverGoesNegative() {
        listOf(20000L to "20", 19999L to "20", 1001L to "2", 1000L to "1", 1L to "1", 0L to "0", -1L to "0").forEach { (left, label) ->
            val waiting = ready.copy(iphone = ready.iphone.copy(started = true, carWaitUntilMillis = now + left,
                stepSinceMillis = now - 1000)).step(CARPLAY_START)
            assertEquals(WAITING, waiting.state)
            assertEquals(StepText.of(R.string.path_car_screen_wait, label), waiting.detail)
            assertEquals(START_NOW, waiting.action)
            assertEquals(R.string.path_action_start_now, waiting.actionLabel)
            assertEquals(now - 1000, waiting.sinceMillis)
        }
        // A saved screen does not invent a wait; the runtime policy provides the deadline.
        assertEquals(BLOCKED, ready.copy(car = ready.car.copy(savedScreen = "1280×720")).step(CARPLAY_START).state)
    }

    @Test fun wirelessStagesAdvanceOnlyTheirOwnSteps() {
        val expected = listOf(LinkStep.STARTING to (CARPLAY_START to WORKING),
            LinkStep.WAITING_IPHONE to (IPHONE_BLUETOOTH to WAITING),
            LinkStep.BLUETOOTH to (IPHONE_BLUETOOTH to WORKING), LinkStep.AUTH to (IPHONE_AUTH to WORKING),
            LinkStep.WIFI_SENT to (IPHONE_WIFI to WORKING), LinkStep.JOINING to (IPHONE_WIFI to WORKING),
            LinkStep.SESSION to (SESSION to WORKING), LinkStep.VIDEO to (SESSION to OK))
        expected.forEach { (stage, active) ->
            val input = phone(stage)
            assertEquals(stage.name, active.second, input.step(active.first).state)
            assertTrue(path(input).running)
            assertFalse(path(input).canStart)
            if (active.second != OK) assertEquals(now - 5000, input.step(active.first).sinceMillis)
            val steps = path(input).section(PathSection.IPHONE)!!.steps
            val activeIndex = steps.indexOfFirst { it.id == active.first }
            assertTrue(steps.take(activeIndex).all { it.state == OK })
            assertTrue(steps.drop(activeIndex + 1).all { it.state == BLOCKED })
        }
        assertEquals(StepText.of(R.string.path_iphone_near), phone(LinkStep.WAITING_IPHONE).step(IPHONE_BLUETOOTH).detail)
        assertEquals(StepText.of(R.string.path_connecting_iphone, "iPhone 15"), phone(LinkStep.BLUETOOTH).step(IPHONE_BLUETOOTH).detail)
        val auth = phone(LinkStep.AUTH).let { it.copy(iphone = it.iphone.copy(stepDetail = "Authentication accepted")) }
        assertEquals(StepText.raw("Authentication accepted"), auth.step(IPHONE_AUTH).evidence)
        assertEquals(StepText.of(R.string.path_iphone_joining_wifi, "OpenXPlay"), phone(LinkStep.JOINING).step(IPHONE_WIFI).detail)
        assertEquals(StepText.of(R.string.path_iphone_joining_wifi, "Wi-Fi Direct"), phone(LinkStep.WIFI_SENT,
            ready.copy(relay = ready.relay.copy(hotspotSsid = null))).step(IPHONE_WIFI).detail)
        assertEquals(WAITING, phone(LinkStep.ENDED).step(SESSION).state)
        assertEquals(StepText.of(R.string.path_reconnecting), phone(LinkStep.ENDED).step(SESSION).detail)
        assertFalse(path(ready.copy(iphone = ready.iphone.copy(step = LinkStep.ENDED))).running)
        assertTrue(path(ready.copy(iphone = ready.iphone.copy(started = true))).running)
    }

    @Test fun wiredStagesAndUsbRoleAreReachable() {
        val wired = ready.copy(setup = ready.setup.copy(network = NetworkPlan.USB))
        assertEquals(OK, wired.step(USB_HOST).state)
        assertEquals(StepText.of(R.string.path_usb_host_mode), wired.step(USB_HOST).detail)
        listOf(UsbRole.DISCONNECTED, UsbRole.UNKNOWN).forEach { role ->
            assertEquals(WAITING, wired.copy(iphone = wired.iphone.copy(usb = role)).step(USB_HOST).state)
        }
        val device = wired.copy(iphone = wired.iphone.copy(usb = UsbRole.DEVICE)).step(USB_HOST)
        assertEquals(FAILED, device.state)
        assertEquals("usb_role", device.code)
        assertEquals(R.string.path_explain_usb_role, device.explanation)
        listOf(Triple(LinkStep.USB_DETECT, USB_IPHONE, WAITING), Triple(LinkStep.USB_PERMISSION, USB_IPHONE, ACTION),
            Triple(LinkStep.USB_TRUST, USB_TRUST, WAITING), Triple(LinkStep.USB_CONTROL, USB_CONTROL, WORKING),
            Triple(LinkStep.SESSION, SESSION, WORKING), Triple(LinkStep.VIDEO, SESSION, OK)).forEach { (stage,id,state) ->
            val input = phone(stage, wired)
            assertEquals(stage.name, state, input.step(id).state)
            val previous = path(input).section(PathSection.IPHONE)!!.steps.takeWhile { it.id != id }
            assertTrue(previous.all { it.state == OK })
        }
        val permission = phone(LinkStep.USB_PERMISSION, wired).step(USB_IPHONE)
        assertEquals(StepText.of(R.string.path_allow_usb), permission.detail)
        assertNull(permission.action)
        assertNull(permission.actionLabel)
        listOf("usb_trust_denied", "iphone_locked").forEach { code ->
            val trust = failed(code, LinkStep.USB_TRUST, input = wired).step(USB_TRUST)
            assertEquals(FAILED, trust.state)
            assertEquals(RETRY, trust.action)
            assertNotNull(trust.explanation)
        }
    }

    @Test fun pairedChoicesHaveTheirOwnCopyAndAction() {
        val cases = listOf(Triple(IphonePick.NONE, R.string.path_pair_iphone, PAIR_IPHONE),
            Triple(IphonePick.UNPAIRED, R.string.path_iphone_unpaired, CHOOSE_IPHONE),
            Triple(IphonePick.MULTIPLE, R.string.path_choose_iphone, CHOOSE_IPHONE),
            Triple(IphonePick.NOT_IPHONE, R.string.path_not_iphone, CHOOSE_IPHONE))
        cases.forEach { (pick,res,action) ->
            val step = ready.copy(iphone = ready.iphone.copy(pick = pick)).step(IPHONE_PAIRED)
            assertEquals(ACTION, step.state)
            assertEquals(res, step.detail!!.res)
            assertEquals(action, step.action)
        }
        assertEquals(listOf("Relay"), ready.copy(iphone = ready.iphone.copy(pick = IphonePick.NONE)).step(IPHONE_PAIRED).detail!!.args)
        assertEquals(listOf("iPhone 15"), ready.copy(iphone = ready.iphone.copy(pick = IphonePick.NOT_IPHONE)).step(IPHONE_PAIRED).detail!!.args)
    }

    @Test fun classifiedFailuresChooseTheirStepAndRetryingIsWorking() {
        val cases = listOf(Triple("hotspot_not_ready", CARPLAY_START, R.string.error_explain_hotspot_not_ready),
            Triple("hotspot_configuration", CARPLAY_START, R.string.error_explain_hotspot_configuration),
            Triple("wifi_off", CARPLAY_START, R.string.path_explain_wifi_off),
            Triple("wifi_reset_required", CARPLAY_START, R.string.path_explain_wifi_reset_required),
            Triple("bluetooth_off", IPHONE_BLUETOOTH, R.string.path_explain_bluetooth_off),
            Triple("iphone_unpaired", IPHONE_BLUETOOTH, R.string.path_explain_iphone_unpaired),
            Triple("rfcomm_timeout", IPHONE_BLUETOOTH, R.string.path_explain_rfcomm_timeout),
            Triple("bluetooth_unusable", IPHONE_BLUETOOTH, R.string.error_explain_bluetooth_unusable),
            Triple("first_tcp_timeout", IPHONE_WIFI, R.string.error_explain_first_tcp_timeout))
        cases.forEach { (code,id,explanation) ->
            listOf(false, true).forEach { retrying ->
                val input = failed(code, LinkStep.JOINING, retrying)
                val step = input.step(id)
                assertEquals(code, if (retrying) WORKING else FAILED, step.state)
                assertEquals(code, step.code)
                assertEquals(explanation, step.explanation)
                assertEquals(now - 7000, step.sinceMillis)
                assertEquals(listOf("first", "second"), step.trail)
                assertEquals(RETRY, step.action)
                assertEquals(if (retrying) PathTone.PROGRESS else PathTone.ERROR, path(input).tone)
                if (retrying) {
                    assertEquals(StepText.of(R.string.path_retrying, code), step.detail)
                    assertFalse(path(input).diagnostic.contains(" | E "))
                }
            }
        }
        val password = failed("first_tcp_timeout", LinkStep.JOINING).step(IPHONE_WIFI)
        assertEquals(StepText.of(R.string.path_iphone_did_not_join), password.detail)
        val off = failed("hotspot_configuration", LinkStep.STARTING, input = ready.copy(relay = ready.relay.copy(hotspotOn = false))).step(CARPLAY_START)
        assertEquals(FAILED, off.state)
        assertEquals("hotspot_configuration", off.code)
        assertEquals(OPEN_HOTSPOT, off.action)
        val unknown = failed("unknown", LinkStep.AUTH).step(IPHONE_AUTH)
        assertEquals(StepText.raw("Failure: unknown"), unknown.detail)
        assertNull(unknown.explanation)
        assertEquals(RETRY, unknown.action)
        assertEquals(FAILED, failed("unknown", LinkStep.SESSION).step(SESSION).state)
    }

    @Test fun carlinkPhasesPinAndFailuresFollowTheChain() {
        val scan = carlink(CarlinkInput(CarlinkPhase.SCANNING)).step(CARLINK_FIND)
        assertEquals(WAITING, scan.state)
        assertEquals(StepText.of(R.string.path_open_carlink), scan.detail)
        assertEquals(FAILED, carlink(CarlinkInput(CarlinkPhase.UNSUPPORTED)).step(CARLINK_FIND).state)
        val blocked = carlink(CarlinkInput(CarlinkPhase.BLOCKED)).step(CARLINK_FIND)
        assertEquals(FAILED, blocked.state)
        assertEquals("carlink_blocked", blocked.code)
        assertEquals(FIX_SETTINGS, blocked.action)
        assertEquals(BLOCKED, carlink(CarlinkInput(CarlinkPhase.NEEDS_PERMISSIONS)).step(CARLINK_FIND).state)
        listOf(CarlinkPhase.EXCHANGING to R.string.path_exchanging, CarlinkPhase.JOINING to R.string.path_joining_car_wifi,
            CarlinkPhase.AUTHENTICATING to R.string.path_verifying).forEach { (phase,res) ->
            val value = carlink(CarlinkInput(phase, sinceMillis = now - 1000, carName = "Car"))
            assertEquals(OK, value.step(CARLINK_FIND).state)
            assertEquals(StepText.raw("Car"), value.step(CARLINK_FIND).evidence)
            assertEquals(WORKING, value.step(CARLINK_LINK).state)
            assertEquals(StepText.of(res), value.step(CARLINK_LINK).detail)
            assertEquals(now - 1000, value.step(CARLINK_LINK).sinceMillis)
            assertEquals(BLOCKED, value.step(CARLINK_PIN).state)
            assertEquals(BLOCKED, value.step(CARLINK_READY).state)
        }
        val pin = carlink(CarlinkInput(CarlinkPhase.PIN, joinMethod = "Wi-Fi", pinPending = true))
        assertEquals(OK, pin.step(CARLINK_LINK).state)
        assertEquals(StepText.raw("Wi-Fi"), pin.step(CARLINK_LINK).evidence)
        assertEquals(ACTION, pin.step(CARLINK_PIN).state)
        assertEquals(ENTER_PIN, pin.step(CARLINK_PIN).action)
        assertEquals(BLOCKED, pin.step(CARLINK_READY).state)
        val trusted = carlink(CarlinkInput(CarlinkPhase.NEGOTIATING, trusted = true))
        assertEquals(OK, trusted.step(CARLINK_PIN).state)
        assertEquals(StepText.of(R.string.path_trusted), trusted.step(CARLINK_PIN).detail)
        assertEquals(WORKING, trusted.step(CARLINK_READY).state)
        assertEquals(StepText.of(R.string.path_negotiating), trusted.step(CARLINK_READY).detail)
        val playing = carlink(CarlinkInput(CarlinkPhase.PLAYING, picture = "1280×720 H.265"))
        assertEquals(OK, playing.step(CARLINK_READY).state)
        assertEquals(StepText.raw("1280×720 H.265"), playing.step(CARLINK_READY).evidence)
        val ended = carlink(CarlinkInput(CarlinkPhase.ENDED)).step(CARLINK_READY)
        assertEquals(WAITING, ended.state)
        assertEquals(RECONNECT_CARLINK, ended.action)
        listOf("ble_timeout" to CARLINK_FIND, "gatt_services_failed" to CARLINK_FIND,
            "wifi_join_timeout" to CARLINK_LINK, "auth_rejected" to CARLINK_LINK,
            "heartbeat_timeout" to CARLINK_LINK, "projection_timeout" to CARLINK_READY).forEach { (code,id) ->
            listOf(false, true).forEach { retrying ->
                val value = carlink(CarlinkInput(CarlinkPhase.SCANNING, code = code, error = "error", retrying = retrying,
                    sinceMillis = now - 7000, trail = listOf("scan")))
                val step = value.step(id)
                assertEquals(code, if (retrying) WORKING else FAILED, step.state)
                assertEquals(code, step.code)
                assertEquals(RECONNECT_CARLINK, step.action)
                assertNotNull(step.explanation)
                assertEquals(listOf("scan"), step.trail)
                if (retrying) assertEquals(StepText.of(R.string.path_retrying, code), step.detail)
            }
        }
        val unknown = carlink(CarlinkInput(CarlinkPhase.AUTHENTICATING, code = "unknown:Error", error = "Unknown failure")).step(CARLINK_LINK)
        assertNull(unknown.explanation)
        assertEquals(StepText.raw("Unknown failure"), unknown.detail)
    }

    @Test fun dlnaAndMiracastStatesAndExplanationsAreReused() {
        assertEquals(WORKING, output(CarOutput.DLNA, OutputInput(OutputPhase.SEARCHING)).step(DLNA).state)
        val choose = output(CarOutput.DLNA, OutputInput(OutputPhase.WAITING_USER)).step(DLNA)
        assertEquals(ACTION, choose.state)
        assertEquals(CHOOSE_RENDERER, choose.action)
        listOf(OutputPhase.READY, OutputPhase.WAITING_VIDEO, OutputPhase.STARTING, OutputPhase.PLAYING).forEach { phase ->
            val step = output(CarOutput.DLNA, OutputInput(phase, "Receiver")).step(DLNA)
            assertEquals(OK, step.state)
            assertEquals(StepText.raw("Receiver"), step.evidence)
        }
        val failed = output(CarOutput.DLNA, OutputInput(OutputPhase.FAILED, code = "renderer_not_found", sinceMillis = now - 7000)).step(DLNA)
        assertEquals(FAILED, failed.state)
        assertEquals(R.string.dlna_error_not_found, failed.explanation)
        assertEquals(CHOOSE_RENDERER, failed.action)
        val cast = output(CarOutput.MIRACAST, OutputInput(OutputPhase.WAITING_USER)).step(MIRACAST)
        assertEquals(WAITING, cast.state)
        assertEquals(OPEN_CAST, cast.action)
        val playing = output(CarOutput.MIRACAST, OutputInput(OutputPhase.PLAYING, "Screen")).step(MIRACAST)
        assertEquals(OK, playing.state)
        assertEquals(StepText.raw("Screen"), playing.evidence)
        val error = output(CarOutput.MIRACAST, OutputInput(OutputPhase.FAILED, code = "decoder_failed")).step(MIRACAST)
        assertEquals(FAILED, error.state)
        assertEquals(R.string.miracast_error_decoder, error.explanation)
    }

    @Test fun pictureTeslaAndLiveSummary() {
        val chips = listOf("1920×720", "60 fps", "H.265", "8.1 Mbps")
        val video = phone(LinkStep.VIDEO).copy(live = LiveInput(false, now - 30000, chips))
        assertEquals(StepText.raw("1920×720 · 60 fps"), video.step(SESSION).evidence)
        assertEquals(WAITING, video.step(PICTURE).state)
        assertNull(path(video).live)
        val live = video.copy(live = video.live.copy(pictureOnCar = true))
        assertEquals(OK, live.step(PICTURE).state)
        assertEquals(StepText.raw("1920×720 · 60 fps · H.265 · 8.1 Mbps"), live.step(PICTURE).evidence)
        assertEquals(PathTone.LIVE, path(live).tone)
        assertEquals(LiveSummary(now - 30000, chips), path(live).live)
        assertNull(path(live).focus)
        assertEquals(BLOCKED, ready.step(PICTURE).state)
        assertEquals(StepText.of(R.string.path_picture_after_session), ready.step(PICTURE).detail)
        val notSession = ready.copy(live = LiveInput(pictureOnCar = true))
        assertEquals(StepText.of(R.string.path_picture_after_session), notSession.step(PICTURE).detail)
        assertNotEquals(PathTone.LIVE, path(notSession).tone)
        assertNull(path(notSession).live)
        val tesla = output(CarOutput.TESLA, OutputInput(OutputPhase.READY, "Tesla"), video)
        assertEquals(OK, tesla.step(TESLA).state)
        assertEquals(StepText.raw("Tesla"), tesla.step(TESLA).evidence)
        val waiting = output(CarOutput.TESLA, OutputInput())
        assertEquals(BLOCKED, waiting.step(TESLA).state)
        assertEquals(StepText.of(R.string.path_tesla_after_session), waiting.step(TESLA).detail)
        val failure = output(CarOutput.TESLA, OutputInput(OutputPhase.FAILED, code = "vpn_failed"), video).step(TESLA)
        assertEquals(FAILED, failure.state)
        assertEquals("vpn_failed", failure.code)
    }

    @Test fun focusAndToneFollowPriorityRatherThanFirstNonOkStep() {
        val laterFailure = failed("rfcomm_timeout", LinkStep.BLUETOOTH).copy(
            car = ready.car.copy(web = WebServerInput(WebPhase.STARTING)))
        assertEquals(IPHONE_BLUETOOTH, path(laterFailure).focus!!.id)
        assertEquals(PathTone.ERROR, path(laterFailure).tone)
        val actionBeforeFailedWeb = ready.copy(relay = ready.relay.copy(keepAlive = KeepAliveState.REFUSED),
            car = ready.car.copy(web = WebServerInput(WebPhase.FAILED, code = "web_failed")))
        assertEquals(KEEP_ALIVE, path(actionBeforeFailedWeb).focus!!.id)
        assertEquals(PathTone.ERROR, path(actionBeforeFailedWeb).tone)
        val actionBeforeFailure = laterFailure.copy(relay = laterFailure.relay.copy(keepAlive = KeepAliveState.REFUSED))
        // The failure keeps its own state; the earlier actionable step still comes first as focus.
        assertEquals(KEEP_ALIVE, path(actionBeforeFailure).focus!!.id)
        assertEquals(PathTone.ERROR, path(actionBeforeFailure).tone)
        val missingIdentity = ready.copy(relay = ready.relay.copy(identity = IdentityState.MISSING))
        assertEquals(IDENTITY, path(missingIdentity).focus!!.id)
        val waitingBeforeWorking = ready.copy(car = ready.car.copy(address = null, web = WebServerInput(WebPhase.STARTING)))
        assertEquals(CAR_NETWORK, path(waitingBeforeWorking).focus!!.id)
        assertEquals(PathTone.PROGRESS, path(waitingBeforeWorking).tone)
        assertEquals(CARPLAY_START, path().focus!!.id)
        val videoOnly = phone(LinkStep.VIDEO, ready.copy(setup = ready.setup.copy(outputs = emptySet())))
        val fullyOk = videoOnly.copy(live = LiveInput(pictureOnCar = true))
        assertNull(path(fullyOk).focus)
        val idle = ready.copy(setup = ready.setup.copy(network = NetworkPlan.USB, outputs = emptySet()),
            iphone = ready.iphone.copy(usb = UsbRole.UNKNOWN), relay = ready.relay.copy(identity = IdentityState.READY))
        assertEquals(PathTone.READY, path(idle).tone)
        assertEquals(USB_HOST, path(idle).focus!!.id)
        val unknownRadios = ready.copy(relay = ready.relay.copy(bluetoothOn = null, hotspotOn = null))
        assertEquals(PathTone.READY, path(unknownRadios).tone)
    }

    @Test fun nodeFallbacksAndLiveToneUseTheSpecifiedOrder() {
        val clink = ready.copy(car = ready.car.copy(carlink = CarlinkInput(carName = "Car")))
        assertEquals(PathNodes("iPhone 15", "Relay", "Car"), path(clink).nodes)
        assertEquals("1920×720", path().nodes.car)
        assertEquals("192.168.43.1", path(ready.copy(car = ready.car.copy(screen = null))).nodes.car)
        assertEquals("DLNA", path(ready.copy(car = CarInput(savedScreen = "800×480", dlna = OutputInput(name = "DLNA")))).nodes.car)
        assertNull(path(ready.copy(car = CarInput(savedScreen = "800×480"))).nodes.car)
        val liveWithError = phone(LinkStep.VIDEO).copy(live = LiveInput(pictureOnCar = true),
            car = ready.car.copy(web = WebServerInput(WebPhase.FAILED, code = "web_failed")))
        assertEquals(PathTone.LIVE, path(liveWithError).tone)
    }

    @Test fun carAddressLabelsAreOnlyUsedByTheBrowserOutput() {
        val noScreen = ready.copy(car = ready.car.copy(screen = null))
        CarOutput.entries.forEach { output ->
            val input = noScreen.copy(setup = noScreen.setup.copy(outputs = setOf(output)))
            assertEquals(output.name, if (output == CarOutput.BROWSER) "192.168.43.1" else null, path(input).nodes.car)
        }
        val carlinkOnly = carlink(CarlinkInput(CarlinkPhase.SCANNING), noScreen)
        assertNull(path(carlinkOnly).nodes.car)
        assertEquals("Car", path(carlinkOnly.copy(car = carlinkOnly.car.copy(carlink = CarlinkInput(carName = "Car")))).nodes.car)
        assertEquals("1920×720", path(carlinkOnly.copy(car = carlinkOnly.car.copy(screen = "1920×720"))).nodes.car)
        val both = carlinkOnly.copy(setup = carlinkOnly.setup.copy(outputs = setOf(CarOutput.CARLINK, CarOutput.BROWSER)))
        assertEquals("192.168.43.1", path(both).nodes.car)
        val dlna = output(CarOutput.DLNA, OutputInput(name = "Car receiver"), noScreen)
        assertEquals("Car receiver", path(dlna).nodes.car)
        assertEquals("192.168.43.1", path(dlna.copy(setup = dlna.setup.copy(outputs = setOf(CarOutput.DLNA, CarOutput.BROWSER)))).nodes.car)
        assertNull(path(noScreen.copy(setup = noScreen.setup.copy(outputs = emptySet()))).nodes.car)
    }

    @Test fun threeFullDiagnosticLinesAreExact() {
        assertEquals("R MODE✓ ID✓ PERM✓ BT✓ HS✓ | C NET✓ WEB✓ PAGE✓ | I PAIR✓ CP· RF· IAP· JOIN· SES· | P LIVE· | 0.1.6 14:32:05", path().diagnostic)
        assertEquals("R MODE✓ ID✓ PERM✓ BT✓ HS✓ | C NET✓ WEB✓ PAGE✓ | I PAIR✓ CP✓ RF✓ IAP✓ JOIN✗ SES· | P LIVE· | E first_tcp_timeout 14:31:58 | 0.1.6 14:32:05",
            path(failed("first_tcp_timeout", LinkStep.JOINING)).diagnostic)
        val wired = phone(LinkStep.VIDEO, ready.copy(setup = ready.setup.copy(network = NetworkPlan.USB)))
            .copy(live = LiveInput(pictureOnCar = true))
        assertEquals("R MODE✓ ID✓ PERM✓ | C NET✓ WEB✓ PAGE✓ | I USB✓ CP✓ DEV✓ TRUST✓ CTRL✓ SES✓ | P LIVE✓ | 0.1.6 14:32:05", path(wired).diagnostic)
        val retrying = failed("first_tcp_timeout", LinkStep.JOINING, true)
        assertEquals("R MODE✓ ID✓ PERM✓ BT✓ HS✓ | C NET✓ WEB✓ PAGE✓ | I PAIR✓ CP✓ RF✓ IAP✓ JOIN~ SES· | P LIVE· | 0.1.6 14:32:05", path(retrying).diagnostic)
        val twoFailures = failed("first_tcp_timeout", LinkStep.JOINING).copy(car = ready.car.copy(
            web = WebServerInput(WebPhase.FAILED, code = "web_failed")))
        assertTrue(path(twoFailures).diagnostic.endsWith(" | E web_failed 14:32:05 | 0.1.6 14:32:05"))
        assertTrue(presentPath(ready, ZoneId.of("Asia/Shanghai")).diagnostic.endsWith("0.1.6 22:32:05"))
    }

    @Test fun everyStateNamedInTheStepTableIsReachable() {
        val wired = ready.copy(setup = ready.setup.copy(network = NetworkPlan.USB))
        val samples = buildList {
            add(ready)
            add(ready.copy(setup = ready.setup.copy(valid = false)))
            IdentityState.entries.forEach { add(ready.copy(relay = ready.relay.copy(identity = it))) }
            KeepAliveState.entries.forEach { add(ready.copy(relay = ready.relay.copy(keepAlive = it))) }
            add(ready.copy(relay = ready.relay.copy(permissionsGranted = 0)))
            listOf(true, false, null).forEach { on ->
                add(ready.copy(relay = ready.relay.copy(bluetoothOn = on, hotspotOn = on)))
                add(ready.copy(setup = ready.setup.copy(network = NetworkPlan.SHARED_WIFI),
                    relay = ready.relay.copy(wifiOn = on, sharedWifiSsid = if (on == true) "Home" else null)))
            }
            add(ready.copy(car = ready.car.copy(address = null, pages = 0)))
            add(ready.copy(car = ready.car.copy(pages = 0)))
            WebPhase.entries.forEach { add(ready.copy(car = ready.car.copy(web = WebServerInput(it)))) }
            CarlinkPhase.entries.forEach { phase ->
                add(carlink(CarlinkInput(phase, trusted = phase == CarlinkPhase.NEGOTIATING, pinPending = phase == CarlinkPhase.PIN)))
            }
            listOf("ble_timeout", "wifi_join_timeout", "projection_timeout").forEach { code ->
                add(carlink(CarlinkInput(CarlinkPhase.ENDED, code = code)))
            }
            OutputPhase.entries.forEach { phase ->
                listOf(CarOutput.DLNA, CarOutput.MIRACAST, CarOutput.TESLA).forEach { id ->
                    add(output(id, OutputInput(phase, name = "Car", code = if (phase == OutputPhase.FAILED) "failed" else null)))
                    add(output(id, OutputInput(phase, name = "Car", code = if (phase == OutputPhase.FAILED) "failed" else null), phone(LinkStep.VIDEO)))
                }
            }
            IphonePick.entries.forEach { add(ready.copy(iphone = ready.iphone.copy(pick = it))) }
            UsbRole.entries.forEach { add(wired.copy(iphone = wired.iphone.copy(usb = it))) }
            LinkStep.entries.forEach { stage -> add(phone(stage)); add(phone(stage, wired)) }
            add(ready.copy(iphone = ready.iphone.copy(started = true, carWaitUntilMillis = now + 1000)))
            add(failed("hotspot_configuration", LinkStep.STARTING))
            add(failed("rfcomm_timeout", LinkStep.BLUETOOTH))
            add(failed("first_tcp_timeout", LinkStep.JOINING))
            add(failed("usb_trust_denied", LinkStep.USB_TRUST, input = wired))
            add(phone(LinkStep.VIDEO).copy(live = LiveInput(pictureOnCar = true)))
        }
        val reachable = samples.flatMap { path(it).steps }.groupBy { it.id }.mapValues { (_,steps) -> steps.map { it.state }.toSet() }
        val expected = mapOf(
            MODE to setOf(OK, ACTION), IDENTITY to setOf(OK, WORKING, ACTION), PERMISSIONS to setOf(OK, ACTION),
            KEEP_ALIVE to setOf(WORKING, ACTION), BLUETOOTH to setOf(OK, ACTION, WAITING), WIFI to setOf(OK, ACTION, WAITING),
            HOTSPOT to setOf(OK, ACTION, WAITING), SHARED_WIFI to setOf(OK, ACTION), CAR_NETWORK to setOf(OK, WAITING),
            WEB_SERVER to setOf(OK, WORKING, FAILED), CAR_PAGE to setOf(OK, WAITING),
            CARLINK_FIND to setOf(WAITING, OK, FAILED, BLOCKED), CARLINK_LINK to setOf(WORKING, OK, FAILED),
            CARLINK_PIN to setOf(OK, ACTION), CARLINK_READY to setOf(WORKING, OK, WAITING, FAILED),
            DLNA to setOf(WORKING, ACTION, OK, FAILED), MIRACAST to setOf(WAITING, OK, FAILED),
            IPHONE_PAIRED to setOf(OK, ACTION), USB_HOST to setOf(OK, FAILED, WAITING),
            CARPLAY_START to setOf(BLOCKED, WAITING, WORKING, OK, FAILED), IPHONE_BLUETOOTH to setOf(WAITING, WORKING, FAILED),
            IPHONE_AUTH to setOf(WORKING), IPHONE_WIFI to setOf(WORKING, FAILED), USB_IPHONE to setOf(WAITING, ACTION),
            USB_TRUST to setOf(WAITING, FAILED), USB_CONTROL to setOf(WORKING), SESSION to setOf(WORKING, OK, WAITING),
            TESLA to setOf(BLOCKED, OK, FAILED), PICTURE to setOf(BLOCKED, WAITING, OK))
        expected.forEach { (id,states) -> assertTrue("$id: expected $states, reached ${reachable[id]}", reachable.getValue(id).containsAll(states)) }
    }

    @Test fun everyTitleActionAndExplanationMappingIsCovered() {
        assertEquals(StepId.entries.size, StepId.entries.map { it.titleResource() }.distinct().size)
        assertEquals(listOf(R.string.path_section_relay, R.string.path_section_car, R.string.path_section_iphone,
            R.string.path_section_picture), PathSection.entries.map { it.titleResource() })
        val mappings = mapOf(
            "bluetooth_off" to R.string.path_explain_bluetooth_off, "iphone_unpaired" to R.string.path_explain_iphone_unpaired,
            "rfcomm_timeout" to R.string.path_explain_rfcomm_timeout, "wifi_off" to R.string.path_explain_wifi_off,
            "wifi_reset_required" to R.string.path_explain_wifi_reset_required, "usb_trust_denied" to R.string.path_explain_usb_trust_denied,
            "iphone_locked" to R.string.path_explain_iphone_locked, "usb_role" to R.string.path_explain_usb_role,
            "carlink_blocked" to R.string.path_explain_carlink_blocked, "web_port_busy" to R.string.path_explain_web_port_busy,
            "auth_rejected" to R.string.path_explain_auth_rejected, "auth_timeout" to R.string.path_explain_auth_rejected,
            "pin_cancelled" to R.string.path_explain_auth_rejected, "peer_key_changed" to R.string.path_explain_peer_key_changed,
            "projection_timeout" to R.string.path_explain_projection_timeout, "heartbeat_timeout" to R.string.path_explain_heartbeat_timeout,
            "wifi_link_lost" to R.string.path_explain_heartbeat_timeout, "wifi_join_timeout" to R.string.path_explain_wifi_join,
            "hotspot_not_ready" to R.string.error_explain_hotspot_not_ready, "hotspot_configuration" to R.string.error_explain_hotspot_configuration,
            "first_tcp_timeout" to R.string.error_explain_first_tcp_timeout, "bluetooth_unusable" to R.string.error_explain_bluetooth_unusable,
            "ble_unavailable" to R.string.error_explain_ble_unavailable, "ble_timeout" to R.string.error_explain_ble_timeout,
            "ble_scan_failed" to R.string.error_explain_ble_scan_failed, "gatt_connect_failed" to R.string.error_explain_gatt_connect_failed,
            "gatt_services_failed" to R.string.error_explain_gatt_services_failed, "ble_discovery_failed" to R.string.error_explain_gatt_services_failed,
            "ble_service_missing" to R.string.error_explain_ble_service_missing, "ble_subscribe_failed" to R.string.error_explain_ble_subscribe_failed,
            "ble_cccd_failed" to R.string.error_explain_ble_cccd_failed, "ble_cccd_timeout" to R.string.error_explain_ble_cccd_timeout,
            "ble_write_failed" to R.string.error_explain_ble_write_failed, "ble_request_rejected" to R.string.error_explain_ble_request_rejected,
            "ble_reply_invalid" to R.string.error_explain_ble_reply_invalid, "ble_verify_timeout" to R.string.error_explain_ble_verify_timeout,
            "ble_exchange_timeout" to R.string.error_explain_ble_exchange_timeout, "ble_exchange_failed" to R.string.error_explain_ble_exchange_failed,
            "renderer_not_found" to R.string.dlna_error_not_found, "renderer_unreachable" to R.string.dlna_error_unreachable,
            "renderer_no_avtransport" to R.string.dlna_error_no_avtransport, "renderer_rejected" to R.string.dlna_error_rejected,
            "renderer_stopped" to R.string.dlna_error_stopped, "stream_server_failed" to R.string.dlna_error_server,
            "unsupported_video" to R.string.dlna_error_video, "overlay_denied" to R.string.miracast_allow_overlay,
            "window_failed" to R.string.miracast_error_window, "decoder_failed" to R.string.miracast_error_decoder,
            "display_lost" to R.string.miracast_error_display)
        mappings.forEach { (code,res) -> assertEquals(code, res, pathFailureExplanation(code)) }
        assertNull(pathFailureExplanation(null))
        assertNull(pathFailureExplanation("unknown"))
        val explanations = mapOf(BLUETOOTH to R.string.path_explain_bluetooth, HOTSPOT to R.string.path_explain_hotspot,
            CAR_NETWORK to R.string.path_explain_car_network, CAR_PAGE to R.string.path_explain_car_page,
            CARPLAY_START to R.string.path_explain_carplay_start)
        StepId.entries.forEach { assertEquals(explanations[it], it.explanationResource()) }
        val labels = listOf(R.string.path_action_fix_settings, R.string.path_action_import_identity,
            R.string.path_action_grant_permissions, R.string.path_action_keep_alive_settings,
            R.string.path_action_turn_on, R.string.path_action_turn_on, R.string.path_action_open_hotspot,
            R.string.path_action_choose, R.string.path_action_copy_page_address, R.string.path_action_retry,
            R.string.path_action_enter_pin, R.string.path_action_choose, R.string.path_action_open_cast,
            R.string.path_action_pair_iphone, R.string.path_action_choose, R.string.path_action_start_now, R.string.path_action_retry)
        assertEquals(labels, StepAction.entries.map { it.labelResource() })
    }
}
