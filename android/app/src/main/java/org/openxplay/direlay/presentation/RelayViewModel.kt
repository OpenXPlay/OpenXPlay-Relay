// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-25
package org.openxplay.direlay.presentation

import android.content.Context
import androidx.lifecycle.ViewModel
import androidx.lifecycle.ViewModelProvider
import androidx.lifecycle.viewModelScope
import com.shilapi.xcertplay.AirPlayPersistence
import kotlinx.coroutines.*
import kotlinx.coroutines.flow.*
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import org.openxplay.direlay.background.*
import org.openxplay.direlay.miracast.MiracastSystem
import org.openxplay.direlay.relay.*
import org.openxplay.direlay.settings.*
import org.openxplay.direlay.ui.*
import org.openxplay.stream.core.Viewport

@OptIn(ExperimentalCoroutinesApi::class)
class RelayViewModel(context: Context, private val runtime: RelayRuntime) : ViewModel() {
    private val context = context.applicationContext
    private val worker = Dispatchers.IO.limitedParallelism(1)
    /** Settings and session commands, serialized; sampling never waits behind them. */
    private val commands = Dispatchers.IO.limitedParallelism(1)
    private val revision = MutableStateFlow(0L)
    val hostFlags = MutableStateFlow(HostFlags())
    @Volatile var pendingPairingPrompt: org.openxplay.direlay.admin.PairingWindow.Prompt? = null
    private val draft = MutableStateFlow<RelaySettings?>(null)
    private val producer = HomeProducer(this.context, runtime, ::action)
    private val store get() = runtime.settingsStore
    private val saveLock = Mutex()
    private val backgroundAccess = AndroidBackgroundAccess(this.context)
    private val background = MutableStateFlow<BackgroundStatus?>(null)
    private val packageInfo by lazy { this.context.packageManager.getPackageInfo(this.context.packageName, 0) }
    private val version get() = packageInfo.versionName.orEmpty()
    private val environment by lazy {
        val code = if (android.os.Build.VERSION.SDK_INT >= 28) packageInfo.longVersionCode else packageInfo.versionCode.toLong()
        val build = if (this.context.applicationInfo.flags and android.content.pm.ApplicationInfo.FLAG_DEBUGGABLE != 0) "debug" else "release"
        "$version ($code) $build\nAndroid ${android.os.Build.VERSION.RELEASE} (API ${android.os.Build.VERSION.SDK_INT})\n${android.os.Build.MANUFACTURER} ${android.os.Build.MODEL}"
    }

    private fun <T> sampled(initial: T, read: () -> T): StateFlow<T> = merge(
        flow { while (true) { emit(Unit); delay(500) } },
        revision.map { Unit }, hostFlags.map { Unit }, draft.map { Unit })
        .conflate().map { read() }.distinctUntilChanged().flowOn(worker).stateIn(viewModelScope, SharingStarted.WhileSubscribed(0), initial)

    val readiness: StateFlow<HomeSample?> = sampled(null) { producer.sample(hostFlags.value) }
    val home: StateFlow<HomeContent?> = readiness.map { it?.home }.distinctUntilChanged().flowOn(worker)
        .stateIn(viewModelScope, SharingStarted.WhileSubscribed(0), null)

    private val overlayProducer = HostOverlayProducer(this.context, runtime)
    val overlay: StateFlow<HostOverlaySnapshot?> = sampled(null) { overlayProducer.sample(revision.value, hostFlags.value.resumed) }
    suspend fun sessionAction(action: String) = withContext(commands) {
        try { runtime.sessionActions.action(action) } finally { refresh() }
    }

    val motion = sampled(false) {
        android.provider.Settings.Global.getFloat(this.context.contentResolver, android.provider.Settings.Global.ANIMATOR_DURATION_SCALE, 1f) != 0f
    }
    val sources = ScreenSources(motion = motion,
        settings = sampled<SettingsView?>(null) {
            val settings = store.awaitSnapshot()
            SettingsView(settings, store.developerOverrides.value, store.setupCompleted,
                AirPlayPersistence.loadWirelessEnabled(this.context), runtime.carModels, version, environment, runtime.adminService.enabled, runtime.publicName.view.value.enabled,
                runtime.appliedFrameRate.value, runtime.negotiatedFrameRate.value, runtime.sampleVehicleData)
        },
        car = sampled(CarScreenView()) {
            val rows = runtime.windowRecords.snapshot()
            val detected = rows.firstOrNull()?.let { latest ->
                val screen = ScreenDataKey(latest.screenWidth, latest.screenHeight, latest.densityDpi)
                DetectedCarScreen(screen, rows.filter(screen::matches), runtime.status.receivers.latestInfo(screen)?.toString())
            }
            CarScreenView(runtime.carModels, detected, runtime.savedCanvas, version,
                runtime.ipv4 - listOfNotNull(runtime.teslaAddress?.address).toSet(), runtime.publicName.view.value)
        },
        wirelessPicture = sampled(WirelessPictureView()) {
            val settings = (draft.value ?: store.awaitSnapshot()).copy(wirelessPicture = WirelessPictureMode.AUTO)
            WirelessPictureView(runtime.wirelessPicture(settings), runtime.miracastDisplay?.let { Viewport(it.width, it.height) },
                MiracastSystem.canDrawOverlays(this.context))
        },
        areas = sampled(ViewAreasView()) {
            ViewAreasView(runtime.declaredViewAreas, runtime.viewAreaPreference, runtime.windowRecords.snapshot(),
                runtime.status.receivers.fullscreenExits(), runtime.areaSwitchLog().map { AreaSwitchView(it.time, it.index, it.accepted) })
        },
        debug = sampled(CarlinkDebugView()) {
            val recorder = runtime.carlinkTraceRecorder
            val capture = runtime.carlinkMediaCapture
            CarlinkDebugView(recorder.snapshot().map { TraceLineView(it.type, it.json) }, recorder.dropped.get(),
                recorder.currentFile, recorder.failure, capture.capturing, capture.bytes)
        },
        developer = sampled(DeveloperView()) { developerView(runtime) },
        identity = sampled(org.openxplay.direlay.R.string.direlay_missing) { identityStatusKey(runtime.identityStore.status()) },
        admin = sampled(AdminView()) { AdminView(runtime.adminService.enabled, runtime.adminService.urls) },
        name = sampled(NameView()) { NameView(runtime.publicName.view.value, runtime.publicName.status.value, runtime.publicNameUrl()) },
        iphones = sampled(IphonePickerView()) {
            IphonePickerView(runtime.bondedDevices.read(), runtime.bondedDevices.localName())
        },
        carlinkPicture = combine(runtime.carlinkPictureTrial, runtime.carlinkPictureCanvas, runtime.carlinkPictureEffective,
            ::CarlinkPictureView).stateIn(viewModelScope, SharingStarted.WhileSubscribed(0), CarlinkPictureView()),
        background = background.asStateFlow())

    private fun refreshBackground() { viewModelScope.launch(worker) { background.value = backgroundAccess.read() } }
    suspend fun backgroundIntents(destination: BackgroundDestination) = withContext(worker) { backgroundAccess.intents(destination) }

    fun refresh() { revision.update { it + 1 } }
    fun action(block: () -> Unit) { viewModelScope.launch(commands) { try { block() } finally { refresh() } } }
    suspend fun permissions(): PermissionState = withContext(Dispatchers.IO) { producer.permissionState() }
    private suspend fun persist(change: () -> Unit): AppliedSettings? = withContext(NonCancellable + commands) {
        saveLock.withLock {
            try {
                runCatching {
                    val before = store.awaitSnapshot()
                    val width = store.physicalWidthOverrideMm
                    change()
                    runtime.reloadSettings()
                    val after = store.awaitSnapshot()
                    AppliedSettings(before, after, settingsFeedback(before, after, runtime.carlinkConnected,
                        readiness.value?.home?.presentation?.running == true, width, store.physicalWidthOverrideMm,
                        runtime.carlinkPictureTrial.value))
                }.getOrNull()
            } finally { refresh() }
        }
    }
    fun actions(openBackground: (BackgroundDestination) -> Unit) = ScreenActions(
        save = { after -> persist { SettingsAutoSave(store.awaitSnapshot(), store::save).apply(after, store.wireless(after)) } },
        saveDeveloper = { after, choices -> persist { store.save(after, choices) } },
        readIphones = { withContext(Dispatchers.IO) { IphonePickerView(runtime.bondedDevices.read(), runtime.bondedDevices.localName()) } },
        completeSetup = { action { store.setupCompleted = true } }, draft = { draft.value = it },
        clearHeadUnit = { action { runtime.clearSavedHeadUnit() } }, showArea = { action { runtime.showViewArea(it) } },
        clearWindows = { action { runtime.clearWindowRecords() } },
        clearTrace = { action { runtime.carlinkTraceRecorder.clear(); runtime.carlinkMediaCapture.clear() } },
        capture = { active -> action { if (active) runtime.carlinkMediaCapture.start() else runtime.carlinkMediaCapture.stop() } },
        searchRenderers = { withContext(Dispatchers.IO) { runtime.searchDlnaRenderers() } },
        adminEnabled = { enabled -> action { runtime.adminService.setEnabled(enabled) } },
        resetPassword = { withContext(commands) { runtime.adminService.resetPassword().also { refresh() } } },
        saveName = { edit -> withContext(NonCancellable + commands) { runCatching { runtime.publicName.save(edit) }.getOrNull().also { refresh() } } },
        renewName = { action { runtime.publicName.renewNow() } },
        removeName = { withContext(NonCancellable + commands) { runCatching { runtime.publicName.remove() }.isSuccess.also { refresh() } } },
        useSharedWifi = { ssid -> action { runtime.sharedWifiSelection.use(ssid) } },
        sampleVehicle = { enabled -> action { runtime.setSampleVehicleData(enabled) } },
        clearVehicleHistory = { action { runtime.clearVehicleHistory() } },
        tryCarlinkPicture = { size -> action { runtime.tryCarlinkPicture(size) } },
        clearCarlinkPictureTrial = { restart -> action { runtime.clearCarlinkPictureTrial(restart) } },
        refreshBackground = ::refreshBackground, openBackground = openBackground)

    private val configFiles by lazy {
        org.openxplay.direlay.config.ConfigFiles(this.context, store, runtime.publicName, runtime.tlsStore)
    }
    private val uploader by lazy {
        org.openxplay.direlay.logs.OpenXPlayLogUploader(org.openxplay.direlay.logs.InternetUploadConnection(
            this.context.getSystemService(android.net.ConnectivityManager::class.java)), version,
            org.openxplay.direlay.logs.PreferencesLogUploadStore(
                this.context.getSharedPreferences("log-uploads", Context.MODE_PRIVATE)))
    }
    private val updateState = sampled(org.openxplay.direlay.update.UpdateState(org.openxplay.direlay.update.UpdateChannel.STABLE)) {
        runtime.updates.state.value
    }
    val support = SupportActions(
        ConfigActions({ withContext(Dispatchers.IO) { configFiles.hasPrivateData() } },
            { uri, privacy -> withContext(Dispatchers.IO) { configFiles.export(uri, privacy) } },
            { uri, privacy -> withContext(commands) { configFiles.import(uri, privacy).also { runtime.reloadSettings(); refresh() } } }),
        LogActions({ range -> withContext(Dispatchers.IO) { runtime.logs.export(range) } },
            { file, uri -> withContext(Dispatchers.IO) { runtime.logs.save(file, uri) } },
            { failure -> withContext(Dispatchers.IO) { runtime.logs.failureMessage(failure) } }),
        UploadActions({ withContext(NonCancellable + Dispatchers.IO) { uploader.latestReceipt() } },
            { file, name, progress ->
                val service = withContext(Dispatchers.IO) { uploader }
                service.upload(file, name, progress)
            }),
        UpdateActions(updateState, { action { runtime.updates.check() } }, { channel -> action { runtime.updates.setChannel(channel) } }))

    class Factory(private val context: Context, private val runtime: RelayRuntime) : ViewModelProvider.Factory {
        override fun <T : ViewModel> create(modelClass: Class<T>): T {
            require(modelClass.isAssignableFrom(RelayViewModel::class.java))
            @Suppress("UNCHECKED_CAST")
            return RelayViewModel(context, runtime) as T
        }
    }
}
