// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-14
package org.openxplay.direlay.relay

import android.Manifest
import org.openxplay.direlay.settings.*
import org.openxplay.direlay.carlinkdebug.*
import android.content.Context
import android.content.pm.PackageManager
import android.os.Build
import android.util.Log
import org.openxplay.direlay.network.AndroidNetworkStateReader
import org.openxplay.direlay.network.NetworkReadings
import org.openxplay.direlay.network.NetworkReadiness
import org.openxplay.direlay.clink.*
import org.openxplay.stream.clink.*
import org.openxplay.stream.cast.*
import org.openxplay.direlay.miracast.*
import org.openxplay.direlay.settings.StartupPermissions
import org.openxplay.direlay.settings.RelaySettingsStore
import org.openxplay.direlay.identity.IdentityStore
import org.openxplay.direlay.tls.TlsStore
import org.openxplay.direlay.network.TeslaAddressVpnService
import org.openxplay.direlay.network.phoneIpv4Addresses
import org.openxplay.direlay.publicname.hasTrustedNameConfiguration
import com.shilapi.xcertplay.orchestration.MfiTarget
import com.shilapi.xcertplay.AirPlayPersistence
import org.openxplay.direlay.bridge.DiPlayInput
import org.openxplay.direlay.video.*
import org.openxplay.stream.core.VideoCodec
import org.openxplay.stream.core.DecoderSupport
import org.openxplay.stream.core.VideoCodecDecision
import org.openxplay.stream.core.VideoCodecPolicy
import org.openxplay.stream.core.VideoCodecRestartLimiter
import org.openxplay.stream.core.ProjectionHub
import org.openxplay.stream.web.WebOutput
import org.openxplay.stream.web.WebOutputConfig
import java.io.File
import java.net.Inet4Address
import java.net.NetworkInterface
import java.util.concurrent.Executors
import java.util.concurrent.TimeUnit
import org.openxplay.direlay.bluetooth.AndroidBondedDevices
import org.openxplay.direlay.bluetooth.BondedDevice
import org.openxplay.direlay.bluetooth.BondedDevices
import org.openxplay.direlay.bluetooth.IphoneSelection

/** Application composition root. File, certificate and network discovery work stays off media threads. */
class RelayRuntime(private val context: Context,
    private val clinkDiagnostics: ClinkDiagnostics = AndroidClinkDiagnostics(),
    val bondedDevices: BondedDevices =
        AndroidBondedDevices(context.applicationContext),
    /** One Bluetooth pairing window shared by the app and the admin page. */
    val pairing: org.openxplay.direlay.admin.PairingWindow = org.openxplay.direlay.admin.PairingWindow(
        org.openxplay.direlay.admin.AndroidBluetoothVisibility(context.applicationContext) { false }),
    val keepAlive: RelayKeepAlive = RelayKeepAlive.None,
    val connectionLog: org.openxplay.direlay.logs.ConnectionLog = org.openxplay.direlay.logs.ConnectionLog()) : AutoCloseable {
    @Volatile var iphoneBonds: List<BondedDevice> = emptyList()
        private set
    fun refreshIphoneBonds() { iphoneBonds = bondedDevices.read() }
    fun iphoneSelection(choice: String? = settingsStore.snapshot.value?.iphoneAddress) =
        IphoneSelection.select(choice, iphoneBonds)
    private val traceRecorder = lazy { CarlinkTraceRecorder(checkNotNull(context.getExternalFilesDir("carlink-trace")),
        { line -> Log.i("OXP-CarlinkTrace", line) }, clean = ::cleanTrace) }
    private val traceRedactor by lazy { org.openxplay.direlay.logs.LogRedactor(logSecrets) }
    private fun cleanTrace(line: String) = org.openxplay.direlay.logs.TracePrivacy.redact(line, traceRedactor)
    val carlinkTraceRecorder get() = traceRecorder.value
    private val traceCapture = lazy { CarlinkMediaCapture(checkNotNull(context.getExternalFilesDir("carlink-trace"))) }
    val carlinkMediaCapture get() = traceCapture.value
    val settingsStore = RelaySettingsStore(context.applicationContext).also { it.startObserving() }
    private val networkReader = AndroidNetworkStateReader(context)
    @Volatile var networkReadings = NetworkReadings()
        private set
    fun refreshNetworkReadings(): NetworkReadings = networkReader.read().also { networkReadings = it }
    fun networkReadiness(settings: RelaySettings, readings: NetworkReadings = networkReadings) =
        NetworkReadiness.read(CarlinkConnectionPolicy.enabled(settings), settingsStore.wireless(settings),
            settings.wirelessNetwork.choice == WirelessNetworkChoice.PHONE_HOTSPOT, readings, miracast = settings.miracastOutput)
    private val autoStartBoot = AndroidBootIdentity(context)
    private val autoStartStorage = AndroidAutoStartStorage(context)
    val startRequest: StartRequestGate = ProcessStartRequestGate()
    val autoStartGate: AutoStartGate = PersistentAutoStartGate(autoStartBoot, autoStartStorage) {
        Log.i("OpenXPlay Relay", "Automatic start resumed: new boot")
    }
    val autoStartSessions = AutoStartSessions(context, autoStartGate, autoStartBoot, autoStartStorage)
    @Volatile private var negotiatedFps = 60
    val input = DiPlayInput(
        applyFrameRate = { fps -> negotiatedFps = fps; AirPlayPersistence.saveFps(context, fps) },
        restartCarPlay = { setup.execute { sessionActions.action("restart", manual = false) } },
        automaticHevc = { settingsStore.snapshot.value?.videoCodec == VideoCodecSetting.AUTO && appliedVideoDecision.codec == VideoCodec.H265 },
    )
    private val bundledPlugins = org.openxplay.direlay.plugins.BundledPlugins(context.applicationContext)
    fun installedPlugins() = runCatching {
        org.openxplay.direlay.plugins.RelayPlugins.installed(bundledPlugins.root())
    }.getOrDefault(emptyList())
    val carPlayDiagnostics = CarPlayDiagnostics(secrets = {
        buildList {
            settingsStore.snapshot.value?.let {
                add(it.remoteToken)
                add(it.wirelessNetwork.manualPassphrase)
                addAll(it.wirelessNetwork.sharedWifiNetworks.map { network -> network.passphrase })
            }
            add(AirPlayPersistence.loadManualHotspotPassphrase(context))
            add(AirPlayPersistence.loadExistingWifiPassphrase(context))
        }
    }, timeline = connectionLog)
    val pluginContext = org.openxplay.direlay.plugins.PluginContextObserver(input::context)
    val logSecrets: org.openxplay.direlay.logs.LogSecrets by lazy {
        org.openxplay.direlay.logs.RelayLogSecrets(context.applicationContext, settingsStore, tlsStore) {
            carlinkPins.redactFailureMessage(carlinkFailureRedactor(it))
        }.secrets
    }
    val logs: org.openxplay.direlay.logs.LogFiles by lazy { org.openxplay.direlay.logs.LogFiles(context.applicationContext, logSecrets,
        traceDirectory = { if (settingsStore.snapshot.value?.carlinkTrace == true) context.getExternalFilesDir("carlink-trace") else null }) }
    @Volatile private var configuredPlugins: Pair<Boolean, List<String>> = false to emptyList()

    fun prepareCarPlayFrameRate() { AirPlayPersistence.saveFps(context, negotiatedFps) }
    private fun redactFailureMessage(text: String): String {
        val settings = settingsStore.snapshot.value
        val secrets = buildList {
            settings?.let {
                add(it.remoteToken)
                add(it.wirelessNetwork.manualPassphrase)
                addAll(it.wirelessNetwork.sharedWifiNetworks.map { network -> network.passphrase })
            }
        }
        val redacted = redactFailureText(text, secrets)
        return carlinkPins.redactFailureMessage(carlinkFailureRedactor(redacted))
    }
    private val browserTimeline = org.openxplay.direlay.logs.BrowserTimeline(connectionLog)
    val status = RelayStatus(redactFailure = ::redactFailureMessage, connectionLog = connectionLog, browser = browserTimeline)
    private val setup = Executors.newSingleThreadScheduledExecutor { task ->
        Thread(task, "direlay-runtime").apply { isDaemon = true }
    }
    private val carDisplayMemory = CarDisplayMemory()
    internal val displayPolicy = CarDisplayPolicy(observe = { display ->
        setup.execute { observeWindow(display) }
    }, wholeScreen = { settingsStore.snapshot.value?.let { it.viewAreas && it.displayMode == DisplayMode.AUTOMATIC } == true },
        secondary = carDisplayMemory::isWireless) {
        settingsStore.snapshot.value?.carplayResolution ?: CarPlayResolution.AUTO
    }
    private val hub = ProjectionHub(input, policy = displayPolicy, listener = status)
    private val preferences = context.getSharedPreferences("direlay", Context.MODE_PRIVATE)
    // This separate private file must not invalidate the user-settings snapshot cache.
    private val displayPreferences = context.getSharedPreferences("direlay-car-display", Context.MODE_PRIVATE)
    private val savedHeadUnitStore = SavedHeadUnitStore(displayPreferences)
    @Volatile private var savedHeadUnit: SavedHeadUnit? = null
    @Volatile private var preparedPlanningDisplay: HeadUnitDisplay? = null
    private var clearedReport: HeadUnitDisplay? = null
    @Volatile private var screenPrepared = false
    @Volatile var carPlayScreen: CarPlayScreen? = null
        private set
    @Volatile var carPlayScreenWidthMm: Int? = null
        private set

    @Synchronized private fun loadRememberedDisplay() {
        if (savedHeadUnit == null) savedHeadUnit = savedHeadUnitStore.load()
    }

    @Synchronized private fun rememberCarDisplay() {
        // Pair the measured report with its canvas before another output can renegotiate.
        val (measured, canvas, limit) = synchronized(displayPolicy) {
            Triple(carDisplayMemory.measuredDisplay(displayPolicy.primaryOutput), displayPolicy.carDisplay, displayPolicy.decoderLimit)
        }
        val report = measured?.let(HeadUnitDisplay::from) ?: run {
            if (displayPolicy.primaryDisplay == null) clearedReport = null
            return
        }
        // Clearing while connected must survive periodic refreshes of the same report.
        if (report == clearedReport) return
        clearedReport = null
        val settings = settingsStore.snapshot.value ?: return
        val saved = SavedHeadUnit(canvas ?: return, report, settings.carplayResolution,
            settings.viewAreas && settings.displayMode == DisplayMode.AUTOMATIC, limit)
        if (saved == savedHeadUnit) return
        savedHeadUnitStore.save(saved); savedHeadUnit = saved
        wirelessUpdate { configureDlna() }
    }

    fun clearSavedHeadUnit() { setup.execute { synchronized(this) {
        clearedReport = displayPolicy.primaryDisplay?.let(HeadUnitDisplay::from)
        savedHeadUnitStore.clear(); savedHeadUnit = null
    }; configureDlna() } }

    val savedCanvas: CarPlayScreen get() = synchronized(this) {
        loadRememberedDisplay()
        val settings = settingsStore.snapshot.value ?: return@synchronized CarPlayScreen(CarDisplayFit.default, "default")
        checkNotNull(startupCarPlayScreen(settings.copy(displayMode = DisplayMode.AUTOMATIC), null,
            savedHeadUnit, carModels.model(settings.car)))
    }

    fun wirelessPicture(settings: RelaySettings): WirelessPictureResolution = synchronized(this) {
        loadRememberedDisplay()
        WirelessPicturePolicy.resolve(settings, savedHeadUnit, carModels.model(settings.car))
    }

    private fun planningDisplay(settings: RelaySettings) = planningDisplay(
        displayPolicy.primaryDisplay?.let(HeadUnitDisplay::from), savedHeadUnit, carModels.model(settings.car))

    val sharedWifiSelection = org.openxplay.direlay.settings.SharedWifiSelection()
    private val sharedWifiConnectivity = context.getSystemService(android.net.ConnectivityManager::class.java)
    private val sharedWifiCallback = object : android.net.ConnectivityManager.NetworkCallback() {
        override fun onAvailable(network: android.net.Network) { sharedWifiSelection.wifiChanged() }
        override fun onLost(network: android.net.Network) { sharedWifiSelection.wifiChanged() }
    }
    init {
        sharedWifiConnectivity.registerNetworkCallback(android.net.NetworkRequest.Builder()
            .addTransportType(android.net.NetworkCapabilities.TRANSPORT_WIFI).build(), sharedWifiCallback)
    }
    fun currentSharedWifi(settings: RelaySettings, currentSsid: String? = currentWifiSsid(context)) =
        sharedWifiSelection.resolve(currentSsid, settings.wirelessNetwork.sharedWifiNetworks)

    private val windowPreferences by lazy { context.getSharedPreferences("direlay-window-sizes", Context.MODE_PRIVATE) }
    val carModels by lazy { CarModels.parse(context.assets.open("car-models.json").bufferedReader().use { it.readText() }) }
    val windowRecords by lazy { WindowSizeRecords({ windowPreferences.getString("records", "").orEmpty() },
        { windowPreferences.edit().putString("records", it).apply() }) }
    @Volatile var declaredViewAreas: List<ViewAreaSize> = emptyList()
        private set
    @Volatile var viewAreaPreference: Boolean? = null
        private set
    @Volatile private var activeAreaIndex = 0
    @Volatile private var activeArea: org.openxplay.stream.web.ViewArea? = null
    private val areaListeners = java.util.concurrent.CopyOnWriteArrayList<(org.openxplay.stream.web.ViewArea) -> Unit>()
    private val viewAreaPort = object : org.openxplay.stream.web.ViewAreaPort {
        override fun current() = activeArea
        override fun listen(callback: (org.openxplay.stream.web.ViewArea) -> Unit): AutoCloseable {
            areaListeners.add(callback); return AutoCloseable { areaListeners.remove(callback) }
        }
    }
    private val areaDebounce = ViewAreaSwitchDebounce()
    private var pendingAreaSwitch: java.util.concurrent.ScheduledFuture<*>? = null
    private fun publishArea(index: Int) {
        val areas = declaredViewAreas
        val area = areas.getOrNull(index) ?: return
        activeAreaIndex = index
        val value = org.openxplay.stream.web.ViewArea(0, 0, area.width, area.height, areas[0].width, areas[0].height)
        activeArea = value; updateCarlinkArea(value); areaListeners.forEach { it(value) }
        connectionLog.transition("browser", "view-area", "view_area", "source=view-area-port clients=aggregate view=${value.width}x${value.height} canvas=${value.canvasWidth}x${value.canvasHeight}")
    }
    data class AreaSwitch(val time: Long, val index: Int, val accepted: Boolean)
    private val areaSwitches = ArrayDeque<AreaSwitch>()
    fun areaSwitchLog(): List<AreaSwitch> = synchronized(areaSwitches) { areaSwitches.toList() }
    fun showViewArea(index: Int) { setup.execute {
        pendingAreaSwitch?.cancel(false); areaDebounce.clear(); switchViewArea(index)
    } }
    private fun switchViewArea(index: Int) {
        val accepted = index in declaredViewAreas.indices && input.showViewArea(index)
        if (accepted) publishArea(index)
        synchronized(areaSwitches) {
            if (areaSwitches.size == 32) areaSwitches.removeFirst()
            areaSwitches.addLast(AreaSwitch(System.currentTimeMillis(), index, accepted))
        }
    }
    private fun updateCarlinkArea(area: org.openxplay.stream.web.ViewArea) {
        carlinkVideoOutput?.setCarlinkArea(area.x, area.y, area.width, area.height, area.canvasWidth, area.canvasHeight)
    }
    private fun carlinkPrimary() = carlinkScreen != null && displayPolicy.primaryDisplay === carlinkScreen
    private fun carlinkMatchingArea(settings: RelaySettings): Int? =
        if (carlinkPrimary() && settings.viewAreas && settings.displayMode == DisplayMode.AUTOMATIC)
            carlinkScreen?.let { display -> display.viewport?.let {
                ViewAreaPlanner.match(declaredViewAreas, it.width, it.height, display.fullscreen, display.fullscreenPreferred)
            } } else null
    private var lastCarlinkWindow: List<Int>? = null
    private fun recordCarlinkWindow(display: org.openxplay.stream.core.DisplayInfo?) {
        val viewport = display?.viewport
        if (display == null || viewport == null || display.fullscreen == null) { lastCarlinkWindow = null; return }
        val key = listOf(display.widthPx, display.heightPx, display.densityDpi, viewport.width, viewport.height, if (display.fullscreen == true) 1 else 0)
        if (key == lastCarlinkWindow) return
        lastCarlinkWindow = key
        windowRecords.record(key[0], key[1], key[2], key[3], key[4], display.fullscreen == true, System.currentTimeMillis())
    }
    private var lastWindow: List<Int>? = null
    private fun observeWindow(display: org.openxplay.stream.core.DisplayInfo?) {
        if (display == null) synchronized(this) { clearedReport = null }
        viewAreaPreference = if (display == null) preparedPlanningDisplay?.preferred else display.fullscreenPreferred
        val viewport = display?.viewport
        val areas = declaredViewAreas
        val candidate = if (viewport != null && settingsStore.snapshot.value?.let { it.viewAreas && it.displayMode == DisplayMode.AUTOMATIC } == true &&
            status.session?.phase == org.openxplay.stream.core.SessionPhase.ACTIVE)
            ViewAreaPlanner.match(areas, viewport.width, viewport.height, display.fullscreen, display.fullscreenPreferred)?.takeIf { it != activeAreaIndex } else null
        if (areaDebounce.observe(candidate, android.os.SystemClock.elapsedRealtime())) {
            pendingAreaSwitch?.cancel(false); pendingAreaSwitch = null
            if (candidate != null) pendingAreaSwitch = setup.schedule({
                val index = areaDebounce.ready(android.os.SystemClock.elapsedRealtime())
                areaDebounce.clear()
                if (index != null && settingsStore.snapshot.value?.viewAreas == true && declaredViewAreas === areas && index != activeAreaIndex &&
                    status.session?.phase == org.openxplay.stream.core.SessionPhase.ACTIVE)
                    switchViewArea(index)
            }, 300, java.util.concurrent.TimeUnit.MILLISECONDS)
        }
        if (display != null && display === carlinkScreen) { recordCarlinkWindow(display); return }
        val fullscreen = display?.fullscreen
        if (display == null || viewport == null || fullscreen == null) { lastWindow = null; return }
        val key = listOf(display.widthPx, display.heightPx, display.densityDpi, viewport.width, viewport.height, if (fullscreen) 1 else 0)
        if (key == lastWindow) return
        lastWindow = key
        windowRecords.record(key[0], key[1], key[2], key[3], key[4], fullscreen, System.currentTimeMillis())
    }
    fun clearWindowRecords() { setup.execute { windowRecords.clear(); lastWindow = null; lastCarlinkWindow = null } }
    fun planViewAreas(width: Int, height: Int): List<ViewAreaSize>? {
        val settings = settingsStore.snapshot.value
        if (settings?.viewAreas != true || settings.displayMode != DisplayMode.AUTOMATIC) {
            declaredViewAreas = emptyList()
            activeAreaIndex = 0
            val area = org.openxplay.stream.web.ViewArea(0, 0, width, height, width, height)
            activeArea = area; updateCarlinkArea(area); areaListeners.forEach { it(area) }
            return null
        }
        loadRememberedDisplay()
        val display = if (screenPrepared) preparedPlanningDisplay else planningDisplay(settings)
        viewAreaPreference = display?.preferred
        val areas = planHeadUnitAreas(width, height, display, windowRecords.snapshot(), carModels.model(settings.car))
        return areas.also { declaredViewAreas = it; publishArea(0) }
    }
    private val displayRefit = DisplayRefit()
    @Volatile private var appliedDisplay: DisplayTarget? = null
    /** Shown on the head-unit page next to the page's own version, e.g. "0.1.3 (1003)". */
    private fun appVersion(): String = context.packageManager.getPackageInfo(context.packageName, 0).let {
        @Suppress("DEPRECATION") "${it.versionName} (${if (android.os.Build.VERSION.SDK_INT >= 28) it.longVersionCode else it.versionCode.toLong()})"
    }
    fun displayRestarted() { displayRefit.restarted(android.os.SystemClock.elapsedRealtime()) }

    /** Called on the start worker; freeze the canvas for this CarPlay session. */
    @Synchronized fun prepareCarPlayScreen(settings: RelaySettings): CarPlayScreen? {
        loadRememberedDisplay()
        displayPolicy.refreshDisplay()
        rememberCarDisplay()
        preparedPlanningDisplay = planningDisplay(settings)
        return startupCarPlayScreen(settings, displayPolicy.carDisplay, savedHeadUnit, carModels.model(settings.car)).also {
            settingsStore.prepareDisplay(it?.profile, settings.carplayScreenWidth)
            carPlayScreenWidthMm = AirPlayPersistence.loadWidthPhysicalMm(context)
            appliedDisplay = it?.let { DisplayTarget(it.profile, settings.carplayResolution, settings.carplayScreenWidth, preparedPlanningDisplay?.screen) }
            carPlayScreen = it
            screenPrepared = true
        }
    }

    fun displayMismatch(running: Boolean, settings: RelaySettings): Boolean {
        val live = displayPolicy.primaryDisplay?.let(HeadUnitDisplay::from)
        if (settings.viewAreas || declaredViewAreas.isNotEmpty()) return running && settings.displayMode == DisplayMode.AUTOMATIC &&
            live != null && appliedDisplay != null && carlinkScreenChanged(live.screen, appliedDisplay?.screen,
                carlinkPrimary(), carlinkMatchingArea(settings))
        return (settings.carplayResolution in setOf(CarPlayResolution.SCREEN, CarPlayResolution.NATIVE) || live?.visible == null) && carDisplayMismatch(
            running, settings.displayMode, carPlayScreen?.profile, displayPolicy.carDisplay ?: savedHeadUnit?.canvas)
    }

    /** The CarPlay canvas as "W × H · D dpi" plus where it came from ("car screen", …, "relay screen"). */
    val carPlayScreenSummary: Pair<String, String> get() {
        val settings = settingsStore.snapshot.value
        val selected = if (screenPrepared) carPlayScreen else settings?.let {
            startupCarPlayScreen(it, displayPolicy.carDisplay, savedHeadUnit, carModels.model(it.car))
        }
        if (selected != null) return "${selected.profile.width} × ${selected.profile.height} · ${selected.profile.density} dpi" to selected.source
        val metrics = context.resources.displayMetrics
        return "${metrics.widthPixels} × ${metrics.heightPixels} · ${metrics.densityDpi} dpi" to "relay screen"
    }

    @Volatile var serverState = "Starting web receiver…"
        private set
    @Volatile var serverReason: WebStartReason? = null
        private set
    @Volatile var ipv4: Set<String> = emptySet()
        private set
    /** Addresses a head unit can reach; Wi-Fi Direct groups (p2p-*) only carry the iPhone link. */
    @Volatile var carIpv4: Set<String> = emptySet()
        private set
    @Volatile var receiverUrls: List<String> = emptyList()
        private set
    private val teslaSession = TeslaAddressSession(TeslaAddressVpnService.controller(context.applicationContext))
    val teslaAddress: TeslaAddress? get() = teslaSession.active
    val teslaFailure: String? get() = teslaSession.failure

    fun teslaActivation(settings: RelaySettings, wireless: Boolean = settingsStore.wireless(settings)) =
        teslaBrowserActivation(settings.teslaBrowser, wireless, settings.wirelessNetwork.choice,
            publicName.view.value.hasTrustedNameConfiguration(), settings.carlinkOutput, settings.bothOutputs)

    fun needsVpnConsent(settings: RelaySettings, wireless: Boolean = settingsStore.wireless(settings)): Boolean =
        !wireless || teslaActivation(settings, wireless) == TeslaBrowserActivation.READY

    fun prepareTeslaAddress(settings: RelaySettings, wireless: Boolean) {
        teslaSession.prepare(settings, wireless)
        publicName.refreshAddress()
    }

    fun stopTeslaAddress() {
        try { setup.execute { teslaSession.stop(); publicName.refreshAddress() } }
        catch (_: java.util.concurrent.RejectedExecutionException) { }
    }

    private fun refreshTeslaAddress() {
        val settings = settingsStore.snapshot.value ?: return
        val before = teslaAddress
        teslaSession.tick(autoStartSessions.tracker.running, settings, settingsStore.wireless(settings),
            publicName.view.value.hasTrustedNameConfiguration(), ::phoneIpv4Addresses)
        if (before != teslaAddress) {
            publicName.refreshAddress()
            runCatching { updateWebTls() }.onFailure { Log.w("OpenXPlay Relay", "Tesla TLS update failed", it) }
        }
    }

    private fun webTlsAddresses(): Set<String> {
        val settings = settingsStore.snapshot.value
        val addresses = if (settings == null) carIpv4 else teslaTlsAddresses(carIpv4, settings.teslaBrowser, settings.teslaAddress().orEmpty())
        return addresses + listOfNotNull(teslaAddress?.takeIf { it.mode != TeslaBrowserMode.OFF && !it.mode.usesTrustedName }?.address)
    }
    @Volatile var localPreview = false
        private set
    @Volatile var playCarPlayAudio = false
        private set
    @Volatile var microphoneSource = MicrophoneSource.HEAD_UNIT
        private set
    @Volatile private var configuredWebPort: Int? = null
    @Volatile private var automaticEnabled: Boolean? = null
    @Volatile private var webEnabled = true
    private var networkPermissionAvailable = hasNetworkPermission()
    @Volatile private var closed = false
    private val lifecycle = Any()
    private var web: WebOutput? = null
    val carlinkPins = DialogPinProvider().apply { onSecret = { logSecrets.remember(listOf(it), allowShort = true) } }
    private val carlinkTrust = PrivateClinkTrustStore(context.noBackupFilesDir)
    @Volatile var carlinkState = "Disabled"
        private set
    @Volatile var carlinkDetail = ""
        private set
    @Volatile var carlinkFailed = false
        private set
    private val carlinkTrail = StepTrail()
    // The running attempt's failure code; a kept error shows its own code until this attempt fails too.
    @Volatile private var carlinkAttemptDetail = ""
    @Volatile var carlinkErrorTrail: List<String> = emptyList()
        private set
    @Volatile var carlinkErrorAt: Long? = null
        private set
    @Volatile var carlinkError: String? = null
        private set
    @Volatile private var carlinkFailureRedactor: (String) -> String = { it }
    // Sensor opt-in belongs to a socket generation and waits for the explicit reconnect action.
    private data class CarlinkConfig(val enabled: Boolean, val permitted: Boolean, val name: String, val model: String, val blocked: Boolean, val trace: Boolean, val hevc: ClinkHevcMode, val pictureSize: ClinkPictureSize)
    private var carlinkConfig: CarlinkConfig? = null
    private val carlinkBlock = CarlinkWifiDirectBlock()
    @Volatile private var carlink: ClinkOutput? = null
    @Volatile private var carlinkVideoOutput: TranscodingOutput? = null
    @Volatile var carlinkGeometry: CarlinkGeometry? = null
        private set
    @Volatile var carlinkGeometryMode: VideoCompatibility.Mode? = null
        private set
    @Volatile var vehicleView = org.openxplay.direlay.vehicle.VehicleView()
        private set
    @Volatile private var collectVehicle = false
    private var vehicleRegistration: AutoCloseable? = null
    private val pendingVehicle = linkedMapOf<String, Pair<org.openxplay.stream.core.VehicleData, Long>>()
    private var vehicleDeliveryPending = false
    private var vehicleGeneration = 0L
    private var vehicleSession: Pair<Boolean, Boolean>? = null
    @Volatile var sampleVehicleData = false
        private set
    private var sampleTick = 0L
    fun setSampleVehicleData(enabled: Boolean) { setup.execute {
        val requested = settingsStore.developerOverrides.value.effectiveSampleVehicleData(enabled)
        if (sampleVehicleData == requested) return@execute
        sampleVehicleData = requested
        sampleTick = 0
        configureVehicle()
        if (requested) org.openxplay.direlay.vehicle.sampleVehicleData(sampleTick++).forEach(::queueVehicle)
    } }

    /** Both real and generated data enter the same bounded, per-type 250 ms mailbox. */
    private fun queueVehicle(data: org.openxplay.stream.core.VehicleData) {
        if (!collectVehicle) return
        synchronized(pendingVehicle) {
            pendingVehicle[data.javaClass.name] = data to System.currentTimeMillis()
            if (vehicleDeliveryPending) return
            vehicleDeliveryPending = true
            val generation = vehicleGeneration
            try { setup.schedule({
                val reports = synchronized(pendingVehicle) {
                    if (generation != vehicleGeneration) return@schedule
                    val values = pendingVehicle.values.toList()
                    pendingVehicle.clear(); vehicleDeliveryPending = false
                    values
                }
                if (collectVehicle) vehicleRecorder?.let { recorder ->
                    reports.forEach { (report, at) -> recorder.receive(report, at) }
                    vehicleView = recorder.view()
                }
            }, 250, TimeUnit.MILLISECONDS) }
            catch (_: java.util.concurrent.RejectedExecutionException) { vehicleDeliveryPending = false }
        }
    }
    private var vehicleRecorder: org.openxplay.direlay.vehicle.VehicleRecorder? = null
    val carlinkConnected: Boolean get() = carlink != null && carlinkState !in setOf("Disabled", "ENDED", "Requires Android 10+", "Waiting for permissions")
    val pinRejected: Int get() = carlinkPins.rejected
    val pairingSuccess: Int get() = carlinkPins.successes

    private fun configureVehicle(requestedEnabled: Boolean? = null) {
        if (closed) return
        val enabled = sampleVehicleData || (requestedEnabled ?: settingsStore.awaitSnapshot().vehicleInformation)
        collectVehicle = enabled
        if (vehicleRecorder == null) {
            try {
                vehicleRecorder = org.openxplay.direlay.vehicle.VehicleRecorder(
                    org.openxplay.direlay.vehicle.VehicleSnapshotFile(File(context.noBackupFilesDir, "vehicle-history.bin")),
                    android.os.SystemClock::elapsedRealtime, System::currentTimeMillis)
            } catch (_: Exception) { vehicleView = vehicleView.copy(storageFailed = true); return }
        }
        val recorder = vehicleRecorder ?: return
        val session = enabled to sampleVehicleData
        if (session != vehicleSession) synchronized(pendingVehicle) {
            vehicleGeneration++
            pendingVehicle.clear()
            vehicleDeliveryPending = false
            vehicleSession = session
        }
        recorder.enable(enabled, sampleVehicleData)
        if (!enabled) {
            vehicleRegistration?.close(); vehicleRegistration = null
            synchronized(pendingVehicle) { pendingVehicle.clear() }
        }
        else if (vehicleRegistration == null) vehicleRegistration = hub.addVehicleListener { origin, data ->
            if (origin.outputId == "clink" && !sampleVehicleData) queueVehicle(data)
        }
        vehicleView = recorder.view()
    }
    fun clearVehicleHistory() { setup.execute {
        try { vehicleRecorder?.clear(); vehicleRecorder?.let { vehicleView = it.view() } }
        catch (_: Exception) { vehicleView = vehicleView.copy(storageFailed = true) }
    } }

    private var carlinkLink: ClinkLink? = null
    @Volatile private var carlinkEnded = false
    private var carlinkRetiring = false
    private var carlinkRestartPending = false
    private val carlinkResizePolicy = CarlinkResizePolicy()

    private var webStarting = false
    private var lastWebFailure: String? = null
    private var bindRetry: java.util.concurrent.ScheduledFuture<*>? = null
    private var urlRefresh: java.util.concurrent.ScheduledFuture<*>? = null
    val identityStore = IdentityStore(context.noBackupFilesDir) {
        AirPlayPersistence.saveMfiTarget(context, MfiTarget.LOCAL)
    }
    val tlsStore = TlsStore(context.noBackupFilesDir) { refreshWebTls(); adminService.restartTls() }
    private val sessionController = RelaySessionActions(context.applicationContext, this)
    val sessionActions: RelayActions get() = sessionController
    val networkStart get() = sessionController.networkStart
    fun recheckNetworkStart() = sessionController.recheckNetworkStart()
    fun startAnyway() = sessionController.startAnyway()
    fun cancelNetworkStart() = sessionController.cancelNetworkStart()
    val adminService = org.openxplay.direlay.admin.RelayAdminService(context.applicationContext, this,
        org.openxplay.direlay.admin.AdminActions { org.openxplay.direlay.admin.AdminResult(sessionActions.action(it)) })
    private fun adminHosts(): Set<String> = listOfNotNull(adminService.advertisedHost).toSet()
    /** Trusted HTTPS with a free domain name; off by default, and then it requests nothing and serves nothing new. */
    val publicName = org.openxplay.direlay.publicname.PublicNameService.create(context.applicationContext,
        org.openxplay.direlay.publicname.TlsStoreCertificates(tlsStore),
        context.packageManager.getPackageInfo(context.packageName, 0).versionName.orEmpty(),
        addressOverride = { teslaAddress?.let { teslaPublishedAddress(it.mode, it.address) } }) { publicNameCandidates() }

    private val castNetwork = AndroidCastNetwork(context) {
        ipv4Addresses(includeIphoneLink = false).map { java.net.InetAddress.getByName(it) as Inet4Address }
    }
    private val dlnaLifecycle = RetiringOutputPolicy<DlnaOutputPlan>()
    private val miracastLifecycle = RetiringOutputPolicy<MiracastPicture>()
    @Volatile private var dlnaToken: Any? = null
    @Volatile private var miracastToken: Any? = null
    @Volatile var dlnaState: CastState? = null
        private set
    @Volatile var dlnaRendererName: String? = null
        private set
    @Volatile var dlnaFailure: WirelessDisplayFailure? = null
        private set
    @Volatile var dlnaStatistics: CastStatistics? = null
        private set
    @Volatile var miracastState: MiracastState? = null
        private set
    @Volatile var miracastDisplay: MiracastDisplay? = null
        private set
    @Volatile var miracastFailure: WirelessDisplayFailure? = null
        private set

    init {
        prepareCarPlay()
        hub.addOutput(status)
        hub.start()
        setup.execute {
            loadRememberedDisplay()
            val initial = settingsStore.awaitSnapshot()
            connectionLog.event("relay", "settings_loaded")
            configuredPlugins = initial.plugins to initial.pluginsEnabled
            runCatching { bundledPlugins.root() }
                .onFailure { Log.w("OpenXPlay Relay", "Plugin bundle installation failed", it) }
            microphoneSource = initial.microphoneSource
            localPreview = initial.localPreview
            playCarPlayAudio = initial.playCarPlayAudio
            automaticEnabled = initial.autoStartWhenReady
            configuredWebPort = initial.webPort
            webEnabled = !initial.carlinkOutput || initial.bothOutputs
            updateKeepAlive(initial)
            prepareCarPlay()
            ipv4 = ipv4Addresses(); carIpv4 = ipv4Addresses(includeIphoneLink = false)
            startWeb()
            publicName.start()
            configureVehicle()
            configureCarlink()
            configureDlna(); configureMiracast()
            connectionLog.event("relay", "runtime_ready")
        }
        setup.scheduleWithFixedDelay({
            try {
                ipv4 = ipv4Addresses(); carIpv4 = ipv4Addresses(includeIphoneLink = false)
                if (tlsStore.ensureDefault(webTlsAddresses(), adminHosts())) updateWebTls()
                publicName.refreshAddress()
            }
            catch (_: Exception) { /* Retain the last interface snapshot. */ }
            if (!closed && webEnabled && web == null && !webStarting && bindRetry == null) startWeb()
        }, 0, 5, TimeUnit.SECONDS)
    }

    private val sessionTick = setup.scheduleWithFixedDelay({
        refreshNetworkReadings()
        autoStartSessions.refresh()
        refreshTeslaAddress()
        sessionController.observeNetworkSession(autoStartSessions.tracker.running,
            status.session?.phase == org.openxplay.stream.core.SessionPhase.ACTIVE)
        // A session start or stop can block or release Carlink without a settings change.
        if (carlinkBlock.sessionChanged(autoStartSessions.tracker.running)) enqueueCarlink()
    }, 0, 500, TimeUnit.MILLISECONDS)

    private val codecRestarts = VideoCodecRestartLimiter()
    @Volatile var videoDecision = VideoCodecPolicy.resolve(emptyList())
        private set
    @Volatile var appliedVideoDecision = videoDecision
        private set
    @Volatile private var codecRestartPending = false
    @Volatile private var observedCodecSetting: VideoCodecSetting? = null
    @Volatile private var immediateCodecChange = false
    @Volatile private var carlinkDecoders: List<DecoderSupport> = emptyList()
    private var hevcFailed = false
    private var rememberedCodec: VideoCodec? = null
    private var rememberedCodecLoaded = false
    private fun currentVideoCodecDecision(): VideoCodecDecision {
        input.hevcFailure()?.let { failure ->
            if (!hevcFailed) {
                hevcFailed = true
                immediateCodecChange = true
                Log.w("OpenXPlay Relay", "H.265 failed: $failure")
            }
        }
        if (!rememberedCodecLoaded) {
            rememberedCodec = when (preferences.getString("automaticVideoCodec", null)) {
                "H265" -> VideoCodec.H265
                "H264" -> VideoCodec.H264
                else -> null
            }
            rememberedCodecLoaded = true
        }
        return relayVideoCodec(settingsStore.snapshot.value?.videoCodec ?: VideoCodecSetting.AUTO,
            displayPolicy.activeOutputs, carlinkRequiresAvc(carlinkConnected, carlinkDecoders), hevcFailed, rememberedCodec,
            carlinkHevc = carlinkConnected && VideoCompatibility.hasHevc(carlinkDecoders))
    }
    private fun refreshVideoCodec() {
        val setting = settingsStore.snapshot.value?.videoCodec ?: return
        if (observedCodecSetting != null && observedCodecSetting != setting) immediateCodecChange = true
        observedCodecSetting = setting
        val next = currentVideoCodecDecision()
        if (setting == VideoCodecSetting.AUTO && next.reason != "remembered-codec" && next.reason != "no-main-screen-output" && next.codec != rememberedCodec) {
            rememberedCodec = next.codec
            preferences.edit().putString("automaticVideoCodec", next.codec.name).apply()
        }
        if (next != videoDecision) {
            if (next.codec != videoDecision.codec) codecRestarts.changed(android.os.SystemClock.elapsedRealtime())
            videoDecision = next
            Log.i("OpenXPlay Relay", "video codec ${next.codec} reason=${next.reason}")
        }
        if (autoStartSessions.tracker.running && !sessionActions.restarting && !startRequest.pending &&
            !codecRestartPending && next.codec != appliedVideoDecision.codec &&
            (immediateCodecChange || codecRestarts.allow(android.os.SystemClock.elapsedRealtime()))) {
            codecRestartPending = true
            if (sessionActions.action("restart", manual = false) != null) codecRestartPending = false
            else immediateCodecChange = false
        }
        if (next.codec == appliedVideoDecision.codec) immediateCodecChange = false
    }
    private fun refreshDisplay() {
        // A page can connect before the phone declares its areas; reconsider its latest report once active.
        observeWindow(displayPolicy.primaryDisplay)
        displayPolicy.refreshDisplay()
        rememberCarDisplay()
        val settings = settingsStore.snapshot.value ?: return
        val desiredScreen = startupCarPlayScreen(settings, displayPolicy.carDisplay, savedHeadUnit, carModels.model(settings.car))
        val desired = desiredScreen?.let { DisplayTarget(it.profile, settings.carplayResolution, settings.carplayScreenWidth,
            displayPolicy.primaryDisplay?.let(HeadUnitDisplay::from)?.screen?.let { screen ->
                if (!carlinkScreenChanged(screen, appliedDisplay?.screen, carlinkPrimary(), carlinkMatchingArea(settings)))
                    appliedDisplay?.screen ?: screen else screen
            } ?: appliedDisplay?.screen) }
        if (displayRefit.due(android.os.SystemClock.elapsedRealtime(), autoStartSessions.tracker.running,
                settings.displayMode, appliedDisplay, desired, displayPolicy.primaryDisplay?.viewport != null, settings.viewAreas || declaredViewAreas.isNotEmpty()) &&
            !sessionActions.restarting && !startRequest.pending) sessionActions.action("restart", manual = false)
    }
    private val displayTick = setup.scheduleWithFixedDelay({ refreshDisplay(); refreshVideoCodec() }, 0, 250, TimeUnit.MILLISECONDS)

    private val vehicleTick = setup.scheduleWithFixedDelay({
        try {
            if (sampleVehicleData) org.openxplay.direlay.vehicle.sampleVehicleData(sampleTick++).forEach(::queueVehicle)
            if (collectVehicle) vehicleRecorder?.let { it.tick(); vehicleView = it.view() }
        } catch (_: Exception) { vehicleView = vehicleView.copy(storageFailed = true) }
    }, 1, 1, TimeUnit.SECONDS)

    fun prepareCarPlay(chooseVideoCodec: Boolean = false) {
        input.microphoneAvailable(microphoneSource == MicrophoneSource.HEAD_UNIT &&
            context.checkSelfPermission(Manifest.permission.RECORD_AUDIO) == PackageManager.PERMISSION_GRANTED)
        if (!chooseVideoCodec) return
        carlinkBlock.carPlayStarting(AirPlayP2pReservation(context).reservedByCarPlay())
        enqueueCarlink()
        synchronized(this) {
            val decision = currentVideoCodecDecision()
            observedCodecSetting = settingsStore.snapshot.value?.videoCodec
            immediateCodecChange = false
            videoDecision = decision
            appliedVideoDecision = decision
            codecRestartPending = false
            AirPlayPersistence.saveHevcEnabled(context, decision.codec == VideoCodec.H265)
            Log.i("OpenXPlay Relay", "video codec ${decision.codec} reason=${decision.reason}")
        }
    }

    fun setLocalPreview(enabled: Boolean) {
        localPreview = enabled
        preferences.edit().putBoolean("localPreview", enabled).apply()
    }

    fun reloadSettings() {
        val requested = settingsStore.snapshot.value ?: return
        if (!settingsStore.developerOverrides.value.active) {
            setSampleVehicleData(false)
            if (traceCapture.isInitialized() && carlinkMediaCapture.capturing) carlinkMediaCapture.stop()
        }
        if (automaticEnabled == false && requested.autoStartWhenReady) autoStartSessions.resume("Automatic enabled")
        automaticEnabled = requested.autoStartWhenReady
        val vehicleEnabled = requested.vehicleInformation
        collectVehicle = vehicleEnabled || sampleVehicleData
        try { setup.execute {
            configureVehicle(vehicleEnabled); configureCarlink(); configureDlna(); configureMiracast()
            refreshTeslaAddress()
            publicName.refreshAddress()
        } }
        catch (_: java.util.concurrent.RejectedExecutionException) { }
        val settings = requested
        localPreview = settings.localPreview
        playCarPlayAudio = settings.playCarPlayAudio
        microphoneSource = settings.microphoneSource
        prepareCarPlay()
        val plugins = settings.plugins to settings.pluginsEnabled
        val pluginsChanged = plugins != configuredPlugins
        if (pluginsChanged) {
            configuredPlugins = plugins
            pluginContext.configure(false)
        }
        val port = settings.webPort
        val networkGranted = hasNetworkPermission()
        val enabled = !settings.carlinkOutput || settings.bothOutputs
        if (pluginsChanged || port != configuredWebPort || networkGranted != networkPermissionAvailable || enabled != webEnabled) {
            webEnabled = enabled
            configuredWebPort = port
            networkPermissionAvailable = networkGranted
            restartWeb()
        }
        updateKeepAlive(settings)
    }

    private fun updateKeepAlive(settings: RelaySettings) = synchronized(lifecycle) {
        keepAlive.update(RelayKeepAlivePolicy.needed(!closed, webEnabled, settings.carlinkOutput || settings.bothOutputs,
            settings.dlnaOutput, settings.miracastOutput))
    }

    /** Blocking discovery; callers must use an I/O worker. */
    fun searchDlnaRenderers(): List<CastRenderer> = CastDiscovery(castNetwork).search()

    private fun wirelessUpdate(block: () -> Unit) {
        try { setup.execute { if (!closed) block() } }
        catch (_: java.util.concurrent.RejectedExecutionException) { }
    }

    private fun configureDlna() {
        if (closed) return
        val settings = settingsStore.awaitSnapshot()
        val requested = WirelessOutputPolicy.dlna(settings, wirelessPicture(settings))
        if (requested == null) {
            dlnaState = null; dlnaRendererName = null; dlnaFailure = null; dlnaStatistics = null
        }
        when (dlnaLifecycle.update(requested)) {
            OutputChange.KEEP -> return
            OutputChange.REMOVE -> { hub.removeOutput("cast"); return }
            OutputChange.ADD -> Unit
        }
        val plan = checkNotNull(requested)
        val token = Any(); dlnaToken = token
        dlnaState = CastState.SEARCHING; dlnaRendererName = settings.dlnaRenderer?.name
        dlnaFailure = null; dlnaStatistics = null
        val listener = object : CastListener {
            override fun state(state: CastState, renderer: CastRenderer?) = wirelessUpdate {
                if (dlnaToken !== token) return@wirelessUpdate
                if (state == CastState.ENDED) {
                    hub.removeOutput("cast")
                    dlnaLifecycle.ended(); dlnaToken = null
                    dlnaState = null; dlnaRendererName = null; dlnaStatistics = null; dlnaFailure = null
                    configureDlna()
                } else if (!dlnaLifecycle.retiring) {
                    dlnaState = state; dlnaRendererName = renderer?.friendlyName ?: settingsStore.snapshot.value?.dlnaRenderer?.name
                    if (state == CastState.PLAYING) dlnaFailure = null
                }
            }
            override fun failure(failure: CastFailure) = wirelessUpdate {
                if (dlnaToken === token && !dlnaLifecycle.retiring)
                    dlnaFailure = WirelessDisplayFailure(failure.code, redactFailureMessage(failure.detail), System.currentTimeMillis())
            }
            override fun diagnostic(message: String) = wirelessUpdate {
                if (dlnaToken === token) Log.i("OpenXPlay DLNA", redactFailureMessage(message))
            }
            override fun statistics(statistics: CastStatistics) = wirelessUpdate {
                if (dlnaToken === token && !dlnaLifecycle.retiring) dlnaStatistics = statistics
            }
        }
        try {
            val output = CastOutput(CastOutputConfig(plan.udn, plan.size.width, plan.size.height, fps = 30, audio = true),
                castNetwork, listener)
            hub.addOutput(carDisplayMemory.exclude(TranscodingOutput(output, MediaCodecPipeline.factory, deliveryCodecs = setOf(VideoCodec.H264))))
        } catch (failure: Exception) {
            dlnaToken = null; dlnaLifecycle.ended()
            dlnaFailure = WirelessDisplayFailure("output_failed", failureText(failure, ::redactFailureMessage), System.currentTimeMillis())
        }
    }

    private fun configureMiracast() {
        if (closed) return
        val requested = WirelessOutputPolicy.miracast(settingsStore.awaitSnapshot())
        if (requested == null) { miracastState = null; miracastDisplay = null; miracastFailure = null }
        when (miracastLifecycle.update(requested)) {
            OutputChange.KEEP -> return
            OutputChange.REMOVE -> { hub.removeOutput("miracast"); return }
            OutputChange.ADD -> Unit
        }
        val token = Any(); miracastToken = token
        miracastState = MiracastState.WAITING_FOR_DISPLAY; miracastDisplay = null; miracastFailure = null
        val listener = object : MiracastListener {
            override fun state(state: MiracastState, display: MiracastDisplay?) = wirelessUpdate {
                if (miracastToken !== token) return@wirelessUpdate
                if (state == MiracastState.ENDED) {
                    hub.removeOutput("miracast")
                    miracastLifecycle.ended(); miracastToken = null
                    miracastState = null; miracastDisplay = null; miracastFailure = null
                    configureMiracast()
                } else if (!miracastLifecycle.retiring) {
                    miracastState = state; miracastDisplay = display
                    if (state == MiracastState.SHOWING) miracastFailure = null
                }
            }
            override fun failure(code: String, detail: String) = wirelessUpdate {
                if (miracastToken === token && !miracastLifecycle.retiring)
                    miracastFailure = WirelessDisplayFailure(code, redactFailureMessage(detail), System.currentTimeMillis())
            }
        }
        try { hub.addOutput(carDisplayMemory.exclude(MiracastOutput(context.applicationContext, listener, picture = checkNotNull(requested)))) }
        catch (failure: Exception) {
            miracastToken = null; miracastLifecycle.ended()
            miracastFailure = WirelessDisplayFailure("output_failed", failureText(failure, ::redactFailureMessage), System.currentTimeMillis())
        }
    }

    fun reconnectCarlink() { status.clearFailure(); carPlayDiagnostics.clear(); enqueueCarlink(true) }

    fun cancelCarlinkPairing() {
        carlinkPins.cancel()
        try { setup.execute {
            carlinkRestartPending = false
            carlink?.let {
                carlinkRetiring = !carlinkEnded
                hub.removeOutput(it.id)
            }
            carlinkLink?.disconnect()
            carlink = null; carlinkLink = null
            carlinkState = "ENDED"
        } } catch (_: java.util.concurrent.RejectedExecutionException) { }
    }

    private fun enqueueCarlink(force: Boolean = false) {
        try { setup.execute { configureCarlink(force) } }
        catch (_: java.util.concurrent.RejectedExecutionException) { }
    }

    private fun configureCarlink(force: Boolean = false) {
        if (closed) return
        configureVehicle()
        val settings = settingsStore.awaitSnapshot()
        updateKeepAlive(settings)
        carlinkResizePolicy.enabled = settings.carlinkReconnectOnResize
        if (!settings.carlinkTrace && traceCapture.isInitialized()) carlinkMediaCapture.stop()
        val permitted = StartupPermissions.carlink(Build.VERSION.SDK_INT, true).all {
            context.checkSelfPermission(it) == PackageManager.PERMISSION_GRANTED
        }
        val requested = CarlinkConfig(settings.carlinkOutput || settings.bothOutputs, permitted,
            settings.carlinkDeviceName, settings.carlinkDeviceModel,
            carlinkBlock.blocked(CarlinkConnectionPolicy.enabled(settings),
                settingsStore.carlinkWifiDirectConflict(settings), autoStartSessions.tracker.running), settings.carlinkTrace, settings.carlinkHevc, settings.carlinkPictureSize)
        // Picture size is frozen until the next connection. Experimental choices are captured below,
        // outside this comparison, so editing them never restarts a running link.
        if (!force && requested.copy(pictureSize = carlinkConfig?.pictureSize ?: requested.pictureSize) == carlinkConfig && !carlinkRestartPending) return
        connectionLog.transition("relay", "carlink_output", "carlink_output", if (requested.enabled) "enabled" else "disabled")
        carlinkConfig = requested
        if (carlinkRetiring) { carlinkRestartPending = true; return }
        carlink?.let { output ->
            carlinkRetiring = !carlinkEnded
            carlinkRestartPending = carlinkRetiring
            carlinkPins.retire()
            hub.removeOutput(output.id)
            // ClinkOutput.close stops the session; explicitly cancel a pending platform join too.
            carlinkLink?.disconnect()
            carlink = null
            carlinkVideoOutput = null; carlinkGeometry = null; carlinkGeometryMode = null
            carlinkLink = null
            if (carlinkRetiring) return
        }
        carlinkRestartPending = false
        if (!requested.enabled || carlinkError == null) {
            carlinkDetail = ""
            carlinkFailed = false
            carlinkError = null; carlinkErrorTrail = emptyList(); carlinkErrorAt = null
        }
        carlinkFailureRedactor = { it }
        carlinkVideoDetail = ""
        carlinkVideoFailed = false
        carlinkVideoError = null
        if (!requested.enabled) { carlinkState = "Disabled"; return }
        if (requested.blocked) {
            carlinkState = "Blocked"
            carlinkFailed = true
            carlinkDetail = CarlinkConnectionPolicy.REASON
            return
        }
        if (Build.VERSION.SDK_INT < 29) { carlinkState = "Requires Android 10+"; return }
        if (!permitted) { carlinkState = "Waiting for permissions"; return }
        val id = preferences.getString("clinkDeviceId", null) ?: java.util.UUID.randomUUID().toString().also {
            check(preferences.edit().putString("clinkDeviceId", it).commit())
        }
        val logConnection = ++carlinkLogConnection
        val summary = CarlinkSummaryTrace(
            mirror = { line -> org.openxplay.direlay.logs.LogcatLines.split(line).forEach { Log.i("OXP-Carlink", it) } },
            clean = ::cleanTrace, timeline = { connectionLog.carlink(it, logConnection) },
            car = { connectionLog.carlink("car $it", logConnection) }, emitEvents = !settings.carlinkTrace)
        carlinkSummary?.close(); carlinkSummary = summary
        val full = runCatching { CarlinkTraceRecorder.select(settings.carlinkTrace) { carlinkTraceRecorder } }
            .onFailure { connectionLog.transition("relay", "full_trace_failure", "logging_unavailable", "component=carlink_trace") }
            .getOrDefault(org.openxplay.stream.clink.trace.ClinkTrace.NONE)
        val traceSession = java.util.concurrent.atomic.AtomicLong()
        val trace = org.openxplay.stream.clink.trace.ClinkTrace { event ->
            if (event.session > 0) traceSession.set(event.session)
            runCatching { summary.record(event) }
            if (full !== org.openxplay.stream.clink.trace.ClinkTrace.NONE) runCatching { full.record(event) }
        }
        val diagnostics = TracingClinkDiagnostics(ClinkDiagnostics { event ->
            connectionLog.carlink(event.line(), logConnection)
            carlinkTrail.record(event.line(), event == ClinkStage.LINK_START, System.currentTimeMillis(), System.nanoTime())
            if (event is ClinkDiagnostic.Failure) {
                carlinkAttemptDetail = event.code
                if (carlinkError == null) carlinkDetail = event.code
            }
            clinkDiagnostics.emit(event)
        }, trace, traceSession::get)
        val reservation = AirPlayP2pReservation(context)
        val experiments = settings.experimental.effective(settings.carlinkTrace)
        val link = AndroidClinkLink(context, id, requested.name, requested.model, diagnostics, reservation,
            onSecret = { logSecrets.remember(listOf(it)) }, experimental = experiments)
        val sessionDiagnostics = ClinkSessionDiagnostics(carlinkPins, carlinkTrust, diagnostics)
        carlinkResizePolicy.startSession()
        var resizePlaying = false
        fun observeResize(change: () -> Unit) {
            try { setup.execute {
                if (closed || carlinkLink !== link || carlinkRetiring || carlinkEnded) return@execute
                // Read the live snapshot before resolving a pending reconnect or making a new decision.
                carlinkResizePolicy.enabled = settingsStore.snapshot.value?.carlinkReconnectOnResize == true
                change()
                carlinkResizePolicy.reconnect(resizePlaying)?.let { resize ->
                    Log.i("DiRelayCarlink", "view resized window=${resize.window.width}x${resize.window.height} view=${resize.view.width}x${resize.view.height}; reconnecting")
                    reconnectCarlink()
                }
            } } catch (_: java.util.concurrent.RejectedExecutionException) { }
        }
        val output = ClinkOutput(link, sessionDiagnostics.pinProvider, sessionDiagnostics.trustStore,
            deviceId = id, deviceName = requested.name,
            vehicleData = settings.vehicleInformation,
            hevc = requested.hevc, pictureSize = requested.pictureSize, trace = trace, mediaTap = if (settings.carlinkTrace) carlinkMediaCapture else null,
            listener = object : ClinkListener {
                override fun state(state: ClinkState) {
                    observeResize { resizePlaying = state == ClinkState.PLAYING }
                    sessionDiagnostics.state(state)
                    carlinkState = state.name
                    if (state == ClinkState.NEGOTIATING) {
                        carlinkPins.paired()
                        link.authenticated()
                    }
                    // A reconnected session clears the previous failure shown on the status screens.
                    if (state == ClinkState.PLAYING) {
                        carlinkBackoff.succeeded()
                        carlinkError = null; carlinkErrorTrail = emptyList(); carlinkErrorAt = null
                        carlinkFailed = carlinkVideoFailed
                        carlinkDetail = carlinkVideoDetail
                    }
                    if (state == ClinkState.ENDED) {
                        carlinkStatistics = null; carlinkVideoDecision = null; carlinkScreen = null
                        carlinkGeometry = null; carlinkGeometryMode = null
                        carlinkEnded = true
                        try { setup.execute {
                            carlinkRetiring = false
                            if (carlinkRestartPending) configureCarlink(true) else scheduleCarlinkReconnect()
                        } } catch (_: java.util.concurrent.RejectedExecutionException) { }
                    }
                }
                override fun error(error: Throwable) {
                    carlinkError = failureText(error) { redactFailureMessage(link.redactFailureMessage(it)) }
                    carlinkFailed = true
                    // Code locations only (no messages, which may carry platform details), except protocol codes.
                    val root = generateSequence(error) { it.cause }.last()
                    val code = root.message?.takeIf { it.matches(Regex("[a-z0-9_]{3,64}")) }?.let { " ($it)" }.orEmpty()
                    android.util.Log.w("DiRelayCarlink", "session error ${root.javaClass.simpleName}$code at " +
                        root.stackTrace.take(8).joinToString(" < ") { "${it.className.substringAfterLast('.')}.${it.methodName}:${it.lineNumber}" })
                    sessionDiagnostics.error(error)
                    carlinkDetail = carlinkAttemptDetail.ifBlank { carlinkDetail }
                    carlinkErrorTrail = carlinkTrail.snapshot()
                    carlinkErrorAt = System.currentTimeMillis()
                    val rejected = generateSequence(error) { it.cause }.any { it.message in setOf("auth_rejected", "auth_pin_proof_failed") }
                    if (rejected) carlinkPins.reject(::reconnectCarlink)
                    else carlinkPins.cancel()
                }
                // E3 also puts AUTH diagnostics on the Carlink trail and trace.
                override fun diagnostic(message: String) {
                    link.sessionDiagnostic(message)
                    connectionLog.diagnostic(message)
                }
                override fun statistics(stats: ClinkStatistics) {
                    carlinkStatistics = stats
                    summary.video(stats.codec?.name ?: "unknown", stats.width, stats.height, stats.liveFrames)
                }
                override fun viewSize(width: Int, height: Int) {
                    summary.view(width, height)
                    observeResize { carlinkResizePolicy.viewSize(width, height) }
                }
                override fun profileMismatch(inputCodec: String, decoders: List<String>) {
                    carlinkFailed = true
                    carlinkDetail = (carlinkVideoDetail.takeIf { carlinkVideoFailed }
                        ?: "Head unit cannot decode this video") + "\ninput=$inputCodec decoders=${decoders.joinToString()}"
                    Log.w("DiRelayCarlink", "video encoder mismatch input=$inputCodec decoders=${decoders.joinToString()}")
                }
            })
        carlinkStatistics = null; carlinkVideoDecision = null; carlinkScreen = null
        carlinkEnded = false
        carlinkPins.resumeRequests()
        carlinkState = "CONNECTING"
        carlinkFailureRedactor = link::redactFailureMessage
        carlinkLink = link
        carlink = output
        val videoOutput = TranscodingOutput(output, MediaCodecPipeline.factory, report = { format, decision, failed ->
            carlinkVideoDecision = CarlinkVideoDecision(format, decision)
            val source = when {
                format.codec == org.openxplay.stream.core.VideoCodec.H265 -> "H.265"
                format.codecString.substringAfter("avc1.").startsWith("64") -> "H.264 High"
                format.codecString.substringAfter("avc1.").startsWith("4D", true) -> "H.264 Main"
                else -> "H.264 Baseline"
            }
            val target = when (decision.target?.profile) { 66 -> "Baseline"; 77 -> "Main"; 100 -> "High"; else -> "H.265" }
            carlinkVideoFailed = failed || decision.mode == VideoCompatibility.Mode.UNSUPPORTED
            if (!failed) carlinkVideoError = null
            val reason = VideoCompatibility.reportKey(format, decision, failed).reason
            carlinkVideoDetail = when {
                carlinkVideoFailed -> listOfNotNull("Head unit cannot decode this video", reason, carlinkVideoError).joinToString("\n")
                decision.mode == VideoCompatibility.Mode.TRANSCODE -> "Transcoding $source → $target for the head unit"
                else -> "Video passthrough"
            }
            if (carlinkError == null || carlinkState == ClinkState.PLAYING.name) {
                carlinkFailed = carlinkVideoFailed
                carlinkDetail = carlinkVideoDetail
                if (!carlinkVideoFailed) { carlinkError = null; carlinkErrorTrail = emptyList(); carlinkErrorAt = null }
            }
            Log.i("DiRelayCarlink", "video decision=${decision.mode} input=${format.codecString} size=${format.width}x${format.height} target=${decision.target?.codecString} reason=$reason failed=$failed")
        }, observeCapabilities = { caps ->
            val screen = caps?.displays?.get(org.openxplay.stream.core.ScreenId.MAIN)
            carlinkScreen = screen
            try { setup.execute { recordCarlinkWindow(screen) } }
            catch (_: java.util.concurrent.RejectedExecutionException) { }
            if (screen != null) observeResize { carlinkResizePolicy.windowSize(screen.widthPx, screen.heightPx) }
            val decoders = caps?.decoders.orEmpty()
            if (carlinkDecoders != decoders) {
                carlinkDecoders = decoders
                try { setup.execute { refreshVideoCodec() } }
                catch (_: java.util.concurrent.RejectedExecutionException) { }
            }
            Log.i("DiRelayCarlink", "video decoders=${decoders.joinToString { it.codecString }}")
        // Only advertised HEVC sessions accept H.265; Off keeps H.264 delivery.
        }, deliveryCodecs = if (requested.hevc == ClinkHevcMode.OFF) setOf(org.openxplay.stream.core.VideoCodec.H264)
            else setOf(org.openxplay.stream.core.VideoCodec.H264, org.openxplay.stream.core.VideoCodec.H265),
            onFailure = { _, failure -> carlinkVideoError = failureText(failure) { redactFailureMessage(link.redactFailureMessage(it)) } },
            carlinkGeometry = true, reportGeometry = { geometry, mode ->
                carlinkGeometry = geometry; carlinkGeometryMode = mode
            })
        carlinkVideoOutput = videoOutput
        activeArea?.let(::updateCarlinkArea)
        hub.addOutput(videoOutput)
    }

    private var carlinkSummary: CarlinkSummaryTrace? = null
    private var carlinkLogConnection = 0L
    @Volatile var carlinkStatistics: ClinkStatistics? = null
        private set
    @Volatile var carlinkVideoDecision: CarlinkVideoDecision? = null
        private set
    @Volatile var carlinkScreen: org.openxplay.stream.core.DisplayInfo? = null
        private set
    @Volatile private var carlinkVideoFailed = false
    @Volatile private var carlinkVideoDetail = ""
    @Volatile private var carlinkVideoError: String? = null
    private val carlinkBackoff = AndroidClinkLink.retryBackoff

    /** Reconnects an ended Carlink session with backoff when the user enabled automatic reconnection. */
    private fun scheduleCarlinkReconnect() {
        val settings = settingsStore.snapshot.value ?: return
        if (closed || !settings.carlinkAutoReconnect || !(settings.carlinkOutput || settings.bothOutputs)) return
        val delay = carlinkBackoff.next()
        carlinkTrail.record(ClinkDiagnostic.Retry(delay).line(), false, System.currentTimeMillis(), System.nanoTime())
        connectionLog.carlink(ClinkDiagnostic.Retry(delay).line(), carlinkLogConnection)
        runCatching { clinkDiagnostics.emit(ClinkDiagnostic.Retry(delay)) }
        try { setup.schedule({ if (carlinkEnded && !closed) configureCarlink(true) }, delay, java.util.concurrent.TimeUnit.MILLISECONDS) }
        catch (_: java.util.concurrent.RejectedExecutionException) { }
    }

    private fun hasNetworkPermission(): Boolean = Build.VERSION.SDK_INT < 37 ||
        context.checkSelfPermission("android.permission.ACCESS_LOCAL_NETWORK") == PackageManager.PERMISSION_GRANTED

    fun reportInvalidVideo(type: Int, failure: Exception) {
        try { setup.execute { Log.w("OpenXPlay Relay", "Invalid video stream $type", failure) } }
        catch (_: java.util.concurrent.RejectedExecutionException) { }
    }

    private fun updateWebTls() {
        web?.updateTls(tlsStore.webContext(webTlsAddresses(), adminHosts()))
    }

    private fun refreshWebTls() {
        try { setup.execute {
            if (!closed) runCatching { updateWebTls() }
                .onFailure { Log.w("OpenXPlay Relay", "TLS update failed", it) }
        } } catch (_: java.util.concurrent.RejectedExecutionException) { }
    }

    private fun startWeb(bindAttempt: Int = 0) {
        bindRetry = null
        if (closed || webStarting || web != null) return
        if (!webEnabled) { serverReason = null; serverState = "Web receiver disabled"; return }
        webStarting = true
        val requestedAt = android.os.SystemClock.elapsedRealtime()
        val port = configuredWebPort ?: TlsStore.port(true)
        try {
            carIpv4 = ipv4Addresses(includeIphoneLink = false)
            val ssl = tlsStore.webContext(webTlsAddresses(), adminHosts())
            val root = installReceiver()
            val settings = settingsStore.awaitSnapshot()
            val plugins = if (!settings.plugins) null else runCatching {
                org.openxplay.direlay.plugins.RelayPlugins.configuration(settings, bundledPlugins.root(), context.filesDir)
            }.onFailure { Log.w("OpenXPlay Relay", "Plugin packages unavailable", it) }.getOrNull()
            val output = WebOutput(WebOutputConfig(
                // Includes the local Tesla TUN address; no interface-specific listener is needed.
                bindAddress = "0.0.0.0", port = port, sslContext = ssl,
                staticAssetRoot = root.toPath(), plugins = plugins,
                displaySettings = org.openxplay.direlay.logs.TimelineDisplaySettingsPort(settingsStore, connectionLog), viewArea = viewAreaPort,
                sourceUrl = "https://github.com/OpenXPlay/OpenXPlay-Relay",
                relay = org.openxplay.stream.web.RelayInfo("Android", appVersion()),
            ))
            synchronized(lifecycle) {
                if (closed) return
                web = output
                val observed = org.openxplay.direlay.logs.TimelineWebOutput(output, browserTimeline)
                val destination = if (plugins == null) observed else org.openxplay.direlay.plugins.AndroidPluginOutput(observed)
                hub.addOutput(TranscodingOutput(destination, MediaCodecPipeline.factory))
            }
            output.awaitListening()
            connectionLog.event("relay", "web_listening", "port=$port ms=${android.os.SystemClock.elapsedRealtime() - requestedAt}")
            if (closed) return
            serverReason = null
            lastWebFailure = null
            pluginContext.configure(plugins != null, status.session?.phase == org.openxplay.stream.core.SessionPhase.ACTIVE)
            val assetsState = if (File(root, "receiver/index.html").isFile) "" else "\nReceiver assets missing; build Stream web/dist and rebuild the APK."
            serverState = "TLS enabled · HTTPS/WSS :$port" + assetsState
            urlRefresh = setup.scheduleWithFixedDelay({
                try {
                    val addresses = ipv4Addresses()
                    receiverUrls = (listOf("localhost") + addresses).map { "https://$it:$port/" }
                }
                catch (_: Exception) { /* Retain the last network snapshot. */ }
            }, 0, 5, TimeUnit.SECONDS)
        } catch (failure: Exception) {
            pluginContext.configure(false)
            serverReason = webStartReason(failure, port)
            connectionLog.event("relay", "web_failed", "reason=$serverReason exception=${failure.javaClass.simpleName}")
            val root = generateSequence<Throwable>(failure) { it.cause }.last()
            val reason = "$serverReason: ${root.javaClass.name}: ${root.message}"
            if (reason != lastWebFailure) {
                Log.w("OpenXPlay Relay", "Web receiver start failed: $reason", failure)
                lastWebFailure = reason
            }
            serverState = "Web receiver unavailable: ${failureText(failure, ::redactFailureMessage)}"
            receiverUrls = emptyList()
            urlRefresh?.cancel(false)
            urlRefresh = null
            web?.let { hub.removeOutput(it.id) }
            web = null
            if (!closed && webEnabled) {
                bindRetry = setup.schedule({ startWeb((bindAttempt + 1).coerceAtMost(4)) },
                    webStartRetryDelay(bindAttempt), TimeUnit.MILLISECONDS)
            }
        } finally { webStarting = false }
    }

    private fun installReceiver(): File {
        val version = context.packageManager.getPackageInfo(context.packageName, 0).let {
            "${it.versionCode}:${it.lastUpdateTime}"
        }
        val root = File(context.filesDir, "web")
        if (preferences.getString("receiverVersion", null) == version && File(root, "receiver/index.html").isFile) return root
        val staging = File(context.filesDir, "web-staging")
        check(!staging.exists() || staging.deleteRecursively()) { "Could not clear receiver staging" }
        check(staging.mkdirs()) { "Could not create receiver staging" }
        fun copy(asset: String, target: File) {
            val children = context.assets.list(asset).orEmpty()
            if (children.isNotEmpty()) {
                check(target.isDirectory || target.mkdirs())
                children.forEach { copy("$asset/$it", File(target, it)) }
            } else {
                context.assets.open(asset).use { source -> target.outputStream().use { source.copyTo(it) } }
            }
        }
        context.assets.list("web").orEmpty().forEach { copy("web/$it", File(staging, it)) }
        check(!root.exists() || root.deleteRecursively()) { "Could not replace receiver assets" }
        check(staging.renameTo(root)) { "Could not install receiver assets" }
        check(preferences.edit().putString("receiverVersion", version).commit()) { "Could not persist receiver version" }
        return root
    }

    private fun restartWeb() {
        try {
            setup.execute {
                synchronized(lifecycle) {
                    if (closed) return@execute
                    bindRetry?.cancel(false)
                    bindRetry = null
                    urlRefresh?.cancel(false)
                    urlRefresh = null
                    receiverUrls = emptyList()
                    pluginContext.configure(false)
                    web?.let { hub.removeOutput(it.id) }
                    web = null
                    serverReason = null
                    serverState = "Starting web receiver…"
                }
                startWeb()
            }
        } catch (_: java.util.concurrent.RejectedExecutionException) { }
    }

    /** The head-unit page under the public name, while its certificate is valid and the name points at this relay. */
    fun publicNameUrl(): String? = publicName.status.value.trustedName?.let { "https://$it:${configuredWebPort ?: TlsStore.port(true)}/" }

    fun teslaUrl(): String? {
        if (!webEnabled || !serverState.startsWith("TLS enabled")) return null
        val active = teslaAddress ?: return null
        val host = if (active.mode.usesTrustedName)
            publicName.status.value.takeIf { it.address == active.address }?.trustedName
        else active.address
        if (host == null) return null
        return "https://$host:${configuredWebPort ?: TlsStore.port(true)}/"
    }

    /** One ordering for Home and the running page, including a Wi-Fi Direct-only Tesla session. */
    fun headUnitUrls(): List<String> {
        val car = carIpv4
        val activeAddress = teslaAddress?.address
        val urls = receiverUrls.filterNot { it.contains("://localhost:") || activeAddress != null && it.contains("://$activeAddress:") }
            .filter { url -> car.any { url.contains("://$it:") } }
        val tesla = teslaUrl()
        return (listOfNotNull(tesla, publicNameUrl()?.takeIf { urls.isNotEmpty() || tesla != null }) + urls).distinct()
    }

    /**
     * The car-facing addresses, each marked when it sits on this phone's own hotspot: a tethering
     * interface is no network of ConnectivityManager (Android 15 may list it as a local-only network).
     */
    @Suppress("DEPRECATION")
    private fun publicNameCandidates(): List<org.openxplay.direlay.publicname.AddressCandidate> {
        val car = carIpv4
        val upstream = runCatching {
            sharedWifiConnectivity.allNetworks.filter { network ->
                Build.VERSION.SDK_INT < 35 || sharedWifiConnectivity.getNetworkCapabilities(network)
                    ?.hasCapability(android.net.NetworkCapabilities.NET_CAPABILITY_LOCAL_NETWORK) != true
            }.mapNotNull { sharedWifiConnectivity.getLinkProperties(it)?.interfaceName }.toSet()
        }.getOrNull()
        val marked = runCatching {
            NetworkInterface.getNetworkInterfaces()?.toList().orEmpty().flatMap { network ->
                network.inetAddresses.toList().filterIsInstance<Inet4Address>().mapNotNull { it.hostAddress }.filter { it in car }
                    .map { org.openxplay.direlay.publicname.AddressCandidate(it, hotspot = upstream != null && network.name !in upstream) }
            }
        }.getOrDefault(emptyList())
        return marked + (car - marked.map { it.address }.toSet()).map { org.openxplay.direlay.publicname.AddressCandidate(it, false) }
    }

    /** Without the iPhone link it is the car-facing set: mobile data sits behind the carrier's NAT and is left out too. */
    private fun ipv4Addresses(includeIphoneLink: Boolean = true): Set<String> {
        val cellular = if (includeIphoneLink) emptySet() else cellularInterfaces()
        return NetworkInterface.getNetworkInterfaces()?.toList().orEmpty()
            .filter { it.isUp && !it.isLoopback && (includeIphoneLink || !it.name.startsWith("p2p") && it.name !in cellular && !isCellularName(it.name)) }
            .flatMap { it.inetAddresses.toList() }
            .filterIsInstance<Inet4Address>().filter { !it.isLoopbackAddress && !it.isAnyLocalAddress }
            .mapNotNull { it.hostAddress }.toSortedSet()
    }
    private fun isCellularName(name: String) = listOf("rmnet", "ccmni", "v4-rmnet", "pdp", "wwan").any(name::startsWith)
    @Suppress("DEPRECATION")
    private fun cellularInterfaces(): Set<String> = runCatching {
        sharedWifiConnectivity.allNetworks.mapNotNull { network ->
            val cellular = sharedWifiConnectivity.getNetworkCapabilities(network)
                ?.hasTransport(android.net.NetworkCapabilities.TRANSPORT_CELLULAR) == true
            if (cellular) sharedWifiConnectivity.getLinkProperties(network)?.interfaceName else null
        }.toSet()
    }.getOrDefault(emptySet())

    override fun close() {
        synchronized(lifecycle) {
            if (closed) return
            closed = true
            keepAlive.update(false)
            teslaSession.close()
            sharedWifiConnectivity.unregisterNetworkCallback(sharedWifiCallback)
            publicName.close()
            adminService.close()
            sessionController.close()
            carlinkPins.cancel()
            carlinkLink?.disconnect()
            collectVehicle = false
            vehicleRegistration?.close()
            vehicleRegistration = null
            setup.shutdownNow()
            pluginContext.close()
            if (traceCapture.isInitialized()) carlinkMediaCapture.close()
            if (traceRecorder.isInitialized()) carlinkTraceRecorder.close()
            browserTimeline.close()
            carlinkSummary?.close()
            connectionLog.close()
            settingsStore.close()
            hub.stop()
            input.close()
        }
    }
}
