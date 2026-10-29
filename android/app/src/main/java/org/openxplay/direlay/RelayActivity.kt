// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-14
package org.openxplay.direlay

import androidx.activity.enableEdgeToEdge
import androidx.activity.SystemBarStyle
import org.openxplay.direlay.path.*
import org.openxplay.direlay.presentation.*
import androidx.lifecycle.lifecycleScope
import androidx.lifecycle.repeatOnLifecycle
import androidx.lifecycle.Lifecycle
import kotlinx.coroutines.launch
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import kotlinx.coroutines.flow.collect
import androidx.activity.compose.setContent
import androidx.compose.runtime.getValue
import androidx.compose.runtime.setValue
import android.Manifest
import android.bluetooth.BluetoothAdapter
import android.content.pm.PackageManager
import android.net.VpnService
import android.os.Build
import android.provider.Settings
import com.shilapi.xcertplay.AirPlayPersistence
import com.shilapi.xcertplay.orchestration.MfiTarget
import org.openxplay.direlay.settings.StartupPermissions
import org.openxplay.direlay.miracast.MiracastSystem
import org.openxplay.direlay.admin.PairingVisibilityPolicy
import org.openxplay.direlay.admin.PairingWindow
import android.app.AlertDialog
import android.net.Uri
import android.text.InputType
import android.widget.EditText
import org.openxplay.direlay.storage.MaterialStatus
import org.openxplay.direlay.relay.pathInput
import org.openxplay.direlay.identity.IdentityFiles
import org.openxplay.direlay.identity.IdentityKind
import org.openxplay.direlay.identity.IdentityFileException
import org.openxplay.direlay.identity.IdentityFileProblem
import org.openxplay.direlay.tls.TlsStore
import org.openxplay.direlay.publicname.ConfigError
import org.openxplay.direlay.publicname.ConfigParse
import org.openxplay.direlay.publicname.ConfigProblem
import org.openxplay.direlay.publicname.PublicNameFile
import java.util.concurrent.Executors
import android.content.Intent
import android.content.Context
import android.os.Bundle
import android.os.Handler
import android.os.Looper
import com.shilapi.xcertplay.CarPlayHostActivity
import org.openxplay.stream.core.SessionPhase

class RelayActivity : androidx.activity.ComponentActivity() {
    private val handler = Handler(Looper.getMainLooper())
    private val identityImport by lazy { androidx.lifecycle.ViewModelProvider(this)[IdentityImportState::class.java] }
    private var identityDialog: AlertDialog? = null
    private var dialogVisible by androidx.compose.runtime.mutableStateOf(false)
    private val dialogs = RelayDialogs { dialogVisible = it }
    private val imports = Executors.newSingleThreadExecutor()
    private val runtime get() = (application as DiRelayApplication).runtime
    private val settingsStore get() = runtime.settingsStore
    private var cachedWifi: String? = null
    private var cachedDeviceName: String? = null
    private var networkRefreshPending = false
    private var networkRefreshAgain = false
    private var homeDestination by androidx.compose.runtime.mutableStateOf<String?>(null)
    private var pausingArrival = false
    private var stepNavigation by androidx.compose.runtime.mutableStateOf<StepAction?>(null)
    private var lastNetworkRefresh = Long.MIN_VALUE / 2
    private val presentation by lazy {
        androidx.lifecycle.ViewModelProvider(this, RelayViewModel.Factory(applicationContext, runtime))[RelayViewModel::class.java]
    }
    private var settingsReady by androidx.compose.runtime.mutableStateOf(false)
    private var identityReady = false
    private var materialChecked = false
    @Volatile private var resumed = false
    private val starting get() = runtime.startRequest.pending
    private var requestingPermissions = false
    /** A pairing permission request or "make visible" dialog is on screen; its answer is still to come. */
    private var pairingInFlight = false
    private var pairingCheckPending = false
    private var pendingPairingPrompt: PairingWindow.Prompt?
        get() = presentation.pendingPairingPrompt
        set(value) { presentation.pendingPairingPrompt = value }
    private var bootAutoStart = false
    /** The arrival countdown or the guided setup is on screen; CarPlay then starts only from their buttons. */
    private var guideOpen = false
    private val importOnly get() = intent.getBooleanExtra(EXTRA_IMPORT_IDENTITY, false) ||
        intent.getBooleanExtra(EXTRA_IMPORT_TLS, false) || intent.getBooleanExtra(EXTRA_IMPORT_PUBLIC_NAME, false)
    private var content by androidx.compose.runtime.mutableStateOf<org.openxplay.direlay.ui.HomeContent?>(null)
    /** The runtime's radio conflict unless the user kept the current setup for it. */
    private var radioConflict by androidx.compose.runtime.mutableStateOf<org.openxplay.direlay.network.RadioConflict?>(null)
    private val refreshStatus = object : Runnable {
        override fun run() {
            refreshNetwork()
            publishHostFlags()
            runPairingPrompt()
            showIdentityFollowUp()
            handler.postDelayed(this, 500L)
        }
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) animateSplashExit()
        if (identityImport.uri == null) {
            identityImport.uri = savedInstanceState?.getString("identityUri")?.let(Uri::parse)
            identityImport.kind = savedInstanceState?.getString("identityKind")?.let(IdentityKind::valueOf) ?: IdentityKind.UNKNOWN
            identityImport.awaitingPicker = savedInstanceState?.getBoolean("identityPicker") ?: false
        }
        bootAutoStart = savedInstanceState?.getBoolean("bootAutoStart")
            ?: intent.getBooleanExtra(EXTRA_AUTO_START, false)
        homeDestination = savedInstanceState?.getString("homeDestination")
        stepNavigation = savedInstanceState?.getString("stepNavigation")?.let { runCatching { StepAction.valueOf(it) }.getOrNull() }
        requestingPermissions = savedInstanceState?.getBoolean("requestingPermissions") ?: false
        pairingInFlight = savedInstanceState?.getBoolean("pairingInFlight") ?: false
        // A fresh open with a saved setup can offer the arrival countdown on Home.
        val offerArrival = savedInstanceState == null && runtime.status.session?.phase !in
            setOf(SessionPhase.CONNECTING, SessionPhase.ACTIVE, SessionPhase.RECONNECTING)
        // Hold auto start until restored UI state reports whether the countdown or guide is visible.
        guideOpen = !bootAutoStart && !importOnly
        enableEdgeToEdge(SystemBarStyle.auto(android.graphics.Color.TRANSPARENT, android.graphics.Color.TRANSPARENT),
            SystemBarStyle.auto(android.graphics.Color.TRANSPARENT, android.graphics.Color.TRANSPARENT))
        lifecycleScope.launch {
            repeatOnLifecycle(Lifecycle.State.STARTED) {
                launch { presentation.sources.settings.collect { settingsReady = it != null } }
                launch { presentation.home.collect { content = it } }
                launch { runtime.radioConflict.collect { radioConflict = it?.takeIf { conflict -> conflict != runtime.keptRadioConflict } } }
                launch { presentation.readiness.collect { sample ->
                    if (sample != null && sample.flags == hostFlags() && shouldAutoStart(sample.automatic, hostFlags())) startCarPlay()
                } }
            }
        }
        setContent {
            org.openxplay.direlay.ui.RelayTheme {
                content?.takeIf { settingsReady }?.let { value -> androidx.compose.runtime.CompositionLocalProvider(
                    org.openxplay.direlay.ui.LocalNetworkReadings provides value.networkReadings) {
                    org.openxplay.direlay.ui.RelayApp(value, presentation.sources, presentation.actions, presentation.support, ::shareTrace,
                    onStartStop = {
                        val path = value.path
                        bootAutoStart = false
                        // Asking to start again reopens the choice the user kept for now.
                        if (!path.running) { runtime.keptRadioConflict = null; radioConflict = runtime.radioConflict.value }
                        if (path.running || path.canStart)
                            work { runtime.sessionActions.action(if (path.running) "stop" else "start") }
                        refreshPresentation()
                    },
                    onRestart = {
                        bootAutoStart = false
                        work { runtime.sessionActions.action("restart") }
                    },
                    onAutomatic = { enabled ->
                        imports.execute {
                            settingsStore.save(settingsStore.awaitSnapshot().copy(autoStartWhenReady = enabled))
                            runtime.reloadSettings(); handler.post { refreshPresentation() }
                        }
                    },
                    onSetup = { action -> when (action) {
                        org.openxplay.direlay.ui.SetupAction.PERMISSIONS -> explainPermissions()
                        org.openxplay.direlay.ui.SetupAction.IDENTITY -> openDocument(IDENTITY_KEY, R.string.direlay_choose_key)
                        org.openxplay.direlay.ui.SetupAction.IPHONE -> openPairing()
                        org.openxplay.direlay.ui.SetupAction.SHARED_WIFI, org.openxplay.direlay.ui.SetupAction.SETTINGS, org.openxplay.direlay.ui.SetupAction.CHOOSE_IPHONE -> Unit
                    } },
                    onCopy = { text -> getSystemService(android.content.ClipboardManager::class.java)
                        .setPrimaryClip(android.content.ClipData.newPlainText(getString(R.string.ui_car_screen), text)) },
                    onReconnect = { reconnectCarlink() },
                    onStepAction = ::performStepAction,
                    onRecheck = ::recheckPath,
                    stepNavigation = stepNavigation,
                    homeDestination = homeDestination,
                    onHomeDestinationHandled = { homeDestination = null },
                    onStepNavigationHandled = { stepNavigation = null },
                    importMaterial = { identity -> openDocument(if (identity) IDENTITY_KEY else TLS_FILE, if (identity) R.string.direlay_choose_key else R.string.direlay_choose_tls) },
                    removeMaterial = { identity -> work { if (identity) runtime.identityStore.remove() else runtime.tlsStore.remove() } },
                    importPublicName = { openDocument(PUBLIC_NAME_FILE, R.string.public_name_choose) },
                    openSetup = !bootAutoStart && !importOnly,
                    openLaunch = offerArrival && !bootAutoStart && !importOnly,
                    surfaceBlocked = dialogVisible || requestingPermissions || pairingInFlight,
                    onLater = {
                        if (value.automatic) {
                            pausingArrival = true
                            refreshPresentation()
                            presentation.action {
                                try { runtime.autoStartSessions.pause("not now on Home") }
                                finally { handler.post { pausingArrival = false; refreshPresentation() } }
                            }
                        }
                    },
                    onGuideVisible = { visible -> guideOpen = visible; refreshPresentation() },
                    radioConflict = radioConflict,
                    onRadioConflictAnswer = { conflict, choice ->
                        runtime.keptRadioConflict = conflict
                        radioConflict = null
                        runtime.connectionLog.event("relay", "radio_conflict_answer",
                            "choice=${choice?.name?.lowercase() ?: "keep"} ${conflict.line()}")
                    },
                    )
                } }
            }
        }
        handleStepIntent(intent, restoring = savedInstanceState != null)
        if (savedInstanceState == null) handleImportIntent(intent)
    }

    private fun handleImportIntent(intent: Intent) {
        when {
            intent.getBooleanExtra(EXTRA_IMPORT_IDENTITY, false) -> {
                bootAutoStart = false
                openDocument(IDENTITY_KEY, R.string.direlay_choose_key)
            }
            intent.getBooleanExtra(EXTRA_IMPORT_TLS, false) -> {
                bootAutoStart = false
                openDocument(TLS_FILE, R.string.direlay_choose_tls)
            }
            intent.getBooleanExtra(EXTRA_IMPORT_PUBLIC_NAME, false) -> {
                bootAutoStart = false
                openDocument(PUBLIC_NAME_FILE, R.string.public_name_choose)
            }
        }
    }

    private fun showActivityDialog(builder: AlertDialog.Builder) {
        dialogs.active = resumed && !requestingPermissions && !pairingInFlight
        dialogs.show(builder.create())
    }

    // Finish the icon animation before fading the system splash.
    @android.annotation.TargetApi(Build.VERSION_CODES.S)
    private fun animateSplashExit() {
        splashScreen.setOnExitAnimationListener { view ->
            if (Settings.Global.getFloat(contentResolver, Settings.Global.ANIMATOR_DURATION_SCALE, 1f) == 0f) {
                view.remove()
                return@setOnExitAnimationListener
            }
            val start = view.iconAnimationStart
            val duration = view.iconAnimationDuration
            val remaining = if (start != null && duration != null) {
                java.time.Duration.between(java.time.Instant.now(), start.plus(duration)).toMillis().coerceIn(0L, 600L)
            } else 0L
            view.postDelayed({
                view.alpha = 1f
                view.animate().alpha(0f).setDuration(200L)
                    .setInterpolator(android.view.animation.DecelerateInterpolator())
                    .withEndAction { view.remove() }.start()
            }, remaining)
        }
    }

    private data class NetworkSnapshot(val wifi: String?, val deviceName: String?)

    private fun refreshNetwork(force: Boolean = false) {
        if (networkRefreshPending) { if (force) networkRefreshAgain = true; return }
        networkRefreshPending = true
        // Reading the Wi-Fi name counts as a location access on some systems: refresh it every 5 s,
        // while the session service check stays on every status tick.
        val now = android.os.SystemClock.elapsedRealtime()
        val networkDue = force || now - lastNetworkRefresh >= 5_000L
        if (networkDue) lastNetworkRefresh = now
        imports.execute {
            val network = if (!networkDue) null else {
                val wifi = org.openxplay.direlay.settings.currentWifiSsid(this)
                runtime.refreshIphoneBonds()
                val name = runtime.bondedDevices.localName()
                NetworkSnapshot(wifi, name)
            }
            runtime.autoStartSessions.refresh()
            if (resumed) runtime.recheckNetworkStart()
            handler.post {
                networkRefreshPending = false
                network?.let { cachedWifi = it.wifi; cachedDeviceName = it.deviceName }
                if (networkRefreshAgain) { networkRefreshAgain = false; refreshNetwork(force = true) }
                refreshPresentation()
            }
        }
    }

    private fun hostFlags() = HostFlags(resumed, guideOpen || pausingArrival, requestingPermissions, bootAutoStart,
        importOnly, materialChecked, identityReady, networkRefreshPending, cachedWifi, cachedDeviceName)

    private fun publishHostFlags() { presentation.hostFlags.value = hostFlags() }
    private fun refreshPresentation() { if (!isDestroyed) {
        dialogs.active = resumed && !requestingPermissions && !pairingInFlight
        publishHostFlags(); presentation.refresh()
    } }

    private fun reconnectCarlink() {
        lifecycleScope.launch {
            if (presentation.permissions().missing.isNotEmpty()) explainPermissions()
            else presentation.action { runtime.reconnectCarlink() }
        }
    }

    private fun shareTrace(file: java.io.File) {
        imports.execute {
            val uri = androidx.core.content.FileProvider.getUriForFile(this, "$packageName.carlink-trace", file)
            handler.post {
                if (!isDestroyed) startActivity(Intent.createChooser(Intent(Intent.ACTION_SEND).setType("application/x-ndjson")
                    .putExtra(Intent.EXTRA_STREAM, uri).addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION), null))
            }
        }
    }

    private fun permissionName(permission: String): String = getString(when (permission) {
        Manifest.permission.RECORD_AUDIO -> R.string.direlay_permission_microphone
        Manifest.permission.POST_NOTIFICATIONS -> R.string.direlay_permission_notifications
        Manifest.permission.BLUETOOTH_SCAN, Manifest.permission.BLUETOOTH_CONNECT -> R.string.direlay_permission_bluetooth
        Manifest.permission.NEARBY_WIFI_DEVICES -> R.string.direlay_permission_wifi
        Manifest.permission.ACCESS_FINE_LOCATION -> R.string.direlay_permission_precise_location
        Manifest.permission.ACCESS_COARSE_LOCATION -> R.string.direlay_permission_location
        else -> R.string.direlay_permission_network
    })

    private fun explainPermissions() { lifecycleScope.launch {
        val permissions = presentation.permissions()
        val missing = permissions.missing
        AlertDialog.Builder(this@RelayActivity).setTitle(R.string.direlay_permissions_title)
            .setMessage(getString(R.string.direlay_permissions_explanation) + "\n\n" + (missing.map { permissionName(it) } +
                listOfNotNull(getString(R.string.direlay_vpn_required).takeIf { permissions.consent != null },
                    getString(R.string.path_allow_overlay).takeIf { permissions.overlayNeeded })).joinToString("\n"))
            .setNegativeButton(android.R.string.cancel, null)
            .setNeutralButton(R.string.direlay_android_permissions) { _, _ ->
                openActivity(Intent(Settings.ACTION_APPLICATION_DETAILS_SETTINGS, Uri.parse("package:$packageName")))
            }
            .setPositiveButton(R.string.direlay_continue) { _, _ ->
                requestingPermissions = true
                refreshPresentation()
                if (missing.isNotEmpty()) requestPermissions(missing.toTypedArray(), PERMISSIONS)
                else requestVpnConsent()
            }.let(::showActivityDialog)
    } }

    private fun requestVpnConsent() { lifecycleScope.launch {
        val permissions = presentation.permissions()
        val consent = permissions.consent
        if (permissions.missing.isEmpty() && consent != null) {
            if (!openActivity(consent, VPN_CONSENT)) { requestingPermissions = false; refreshPresentation() }
        }
        else requestOverlayPermission()
    } }

    private fun requestOverlayPermission() { lifecycleScope.launch {
        requestingPermissions = false
        val permissions = presentation.permissions()
        if (permissions.missing.isEmpty() && permissions.consent == null && permissions.overlayNeeded)
            openActivity(MiracastSystem.overlayPermissionIntent(this@RelayActivity))
        refreshPresentation()
    } }

    override fun onRequestPermissionsResult(requestCode: Int, permissions: Array<out String>, grantResults: IntArray) {
        super.onRequestPermissionsResult(requestCode, permissions, grantResults)
        if (requestCode == PERMISSIONS) {
            presentation.action { runtime.reloadSettings() }
            requestVpnConsent()
        }
        if (requestCode == PAIRING_PERMISSION) {
            pairingInFlight = false
            // An interrupted request answers with empty arrays; treat it as declined.
            val granted = grantResults.isNotEmpty() && grantResults.all { it == PackageManager.PERMISSION_GRANTED }
            showPairing(runtime.pairing.permissionResult(granted))
        }
    }

    /** The app's Pair action opens the same pairing window as the admin page; Bluetooth settings stay the fallback. */
    private fun openPairing() {
        pairingCheckPending = true
        imports.execute {
            val opened = runtime.pairing.open().error == null
            val prompt = if (opened) runtime.pairing.take() else null
            handler.post {
                pairingCheckPending = false
                pendingPairingPrompt = prompt
                if (isDestroyed) return@post
                if (!opened || prompt == null && !pairingInFlight) openBluetoothSettings()
                else runPairingPrompt()
            }
        }
    }

    private fun runPairingPrompt() {
        if (!resumed || dialogs.busy || requestingPermissions || pairingInFlight || importOnly) return
        pendingPairingPrompt?.let { pendingPairingPrompt = null; showPairing(it); return }
        if (pairingCheckPending) return
        pairingCheckPending = true
        imports.execute {
            val prompt = runtime.pairing.take()
            handler.post {
                pairingCheckPending = false
                pendingPairingPrompt = prompt
                if (prompt != null && resumed && !dialogs.busy && !requestingPermissions && !pairingInFlight && !importOnly) {
                    pendingPairingPrompt = null
                    showPairing(prompt)
                }
            }
        }
    }

    private fun showPairing(prompt: PairingWindow.Prompt) {
        when (prompt) {
            PairingWindow.Prompt.PERMISSION -> {
                // Asked only now, never at startup: the relay works without Nearby devices.
                val missing = PairingVisibilityPolicy.missing(Build.VERSION.SDK_INT) {
                    checkSelfPermission(it) == PackageManager.PERMISSION_GRANTED
                }
                if (missing.isEmpty()) showPairing(runtime.pairing.permissionResult(true))
                else { pairingInFlight = true; requestPermissions(missing.toTypedArray(), PAIRING_PERMISSION) }
            }
            PairingWindow.Prompt.DIALOG -> {
                var denied = false
                if (openActivity(Intent(BluetoothAdapter.ACTION_REQUEST_DISCOVERABLE)
                    .putExtra(BluetoothAdapter.EXTRA_DISCOVERABLE_DURATION, PairingVisibilityPolicy.dialogSeconds()), PAIRING_DIALOG, onFailure = { denied = it is SecurityException }))
                    pairingInFlight = true
                else showPairing(runtime.pairing.dialogFailed(permission = denied))
            }
            PairingWindow.Prompt.SETTINGS -> if (!openBluetoothSettings()) runtime.pairing.settingsFailed()
        }
        dialogs.active = resumed && !requestingPermissions && !pairingInFlight
    }

    /** OEM-specific pages and denied intents always have a safe system-settings destination. */
    private fun openActivity(intent: Intent, request: Int? = null, onFailure: (Throwable) -> Unit = {}): Boolean = runCatching {
        if (request == null) startActivity(intent) else startActivityForResult(intent, request)
    }.onFailure { failure -> runCatching { startActivity(Intent(Settings.ACTION_SETTINGS)) }; onFailure(failure) }.isSuccess

    private fun openBluetoothSettings(): Boolean = openActivity(Intent(Settings.ACTION_BLUETOOTH_SETTINGS))

    private fun recheckPath() {
        refreshNetwork(force = true)
        refreshPresentation()
    }

    private fun performStepAction(action: StepAction, step: PathStep?) {
        when (action) {
            StepAction.FIX_SETTINGS, StepAction.CHOOSE_IPHONE, StepAction.CHOOSE_SHARED_WIFI,
            StepAction.CHOOSE_RENDERER -> stepNavigation = action
            StepAction.IMPORT_IDENTITY -> openDocument(IDENTITY_KEY, R.string.direlay_choose_key)
            StepAction.GRANT_PERMISSIONS -> explainPermissions()
            StepAction.PAIR_IPHONE -> openPairing()
            StepAction.TURN_ON_BLUETOOTH -> { lifecycleScope.launch {
                val allowed = withContext(Dispatchers.IO) { Build.VERSION.SDK_INT < 31 ||
                    checkSelfPermission(Manifest.permission.BLUETOOTH_CONNECT) == PackageManager.PERMISSION_GRANTED }
                openActivity(Intent(if (allowed) BluetoothAdapter.ACTION_REQUEST_ENABLE else Settings.ACTION_BLUETOOTH_SETTINGS))
            } }
            StepAction.OPEN_WIFI -> openActivity(Intent(if (Build.VERSION.SDK_INT >= 29) Settings.Panel.ACTION_WIFI else Settings.ACTION_WIFI_SETTINGS))
            StepAction.OPEN_HOTSPOT -> runCatching { org.openxplay.direlay.ui.NetworkSettingsLauncher(this).open(true) }
                .onFailure { openActivity(Intent(Settings.ACTION_SETTINGS)) }
            StepAction.KEEP_ALIVE_SETTINGS -> openActivity(Intent(Settings.ACTION_APPLICATION_DETAILS_SETTINGS,
                Uri.parse("package:$packageName")))
            StepAction.COPY_PAGE_ADDRESS -> {
                val text = step?.evidence?.let { it.raw ?: it.res?.let { res -> getString(res, *it.args.toTypedArray()) } }
                if (text != null) getSystemService(android.content.ClipboardManager::class.java)
                    .setPrimaryClip(android.content.ClipData.newPlainText(getString(R.string.ui_car_screen), text))
            }
            StepAction.RECONNECT_CARLINK -> reconnectCarlink()
            StepAction.ENTER_PIN -> recheckPath()
            StepAction.OPEN_CAST -> openActivity(Intent(Settings.ACTION_CAST_SETTINGS))
            StepAction.START_NOW -> work { runtime.sessionActions.action("startNow") }
            StepAction.RETRY -> work { runtime.sessionActions.action("retry") }
        }
    }

    private fun handleStepIntent(intent: Intent, restoring: Boolean = false) {
        val request = RelayLaunchRequest.take(intent, restoring)
        request.destination?.let { homeDestination = it }
        request.stepAction?.let { performStepAction(it, null) }
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        setIntent(intent)
        if (intent.getBooleanExtra(EXTRA_AUTO_START, false)) bootAutoStart = true
        handleStepIntent(intent)
        handleImportIntent(intent)
    }

    private fun startCarPlay() {
        if (starting || !resumed) return
        val boot = bootAutoStart
        work {
            runtime.sessionActions.bootStart = boot
            val error = runtime.sessionActions.start()
            handler.post {
                if (error == null) bootAutoStart = false
                refreshPresentation()
            }
        }
    }

    private fun openDocument(request: Int, title: Int) {
        if ((request == IDENTITY_KEY || request == IDENTITY_CERTIFICATE) &&
            (identityImport.busy || identityImport.awaitingPicker)) return
        val opened = openActivity(Intent(Intent.ACTION_OPEN_DOCUMENT).apply {
            addCategory(Intent.CATEGORY_OPENABLE)
            type = "*/*"
            putExtra(Intent.EXTRA_TITLE, getString(title))
            if (request == IDENTITY_KEY || request == IDENTITY_CERTIFICATE) {
                if (request == IDENTITY_KEY) identityImport.uri = null
                identityImport.awaitingPicker = true
                putExtra(Intent.EXTRA_ALLOW_MULTIPLE, true)
            }
        }, request)
        if (!opened) identityImport.awaitingPicker = false
    }

    private fun read(uri: Uri, limit: Int): ByteArray =
        (contentResolver.openInputStream(uri) ?: error(getString(R.string.direlay_file_unreadable))).use { input ->
            val buffer = ByteArray(limit + 1)
            try {
                var size = 0
                while (size < buffer.size) {
                    val count = input.read(buffer, size, buffer.size - size)
                    if (count < 0) break
                    size += count
                }
                require(size <= limit) { getString(R.string.direlay_file_too_large, limit) }
                buffer.copyOf(size)
            } finally { buffer.fill(0) }
        }

    private fun importIdentity(uris: List<Uri>) {
        val state = identityImport
        val pending = state.uri
        state.awaitingPicker = false
        state.busy = true
        // Only application-owned objects and the retained state are used by the worker after rotation.
        val resolver = applicationContext.contentResolver
        val store = runtime.identityStore
        imports.execute {
            val selected = mutableListOf<IdentityFiles>()
            try {
                if (uris.isEmpty() || uris.size + (if (pending != null) 1 else 0) > 2)
                    throw IdentityFileException(IdentityFileProblem.SELECTION)
                for (uri in listOfNotNull(pending) + uris) {
                    selected.add(IdentityFiles.read(resolver.openInputStream(uri) ?: error("Unreadable identity file")))
                }
                val missing = IdentityFiles.select(selected, followUp = pending != null, store::importFrom)
                if (missing != null) {
                    state.uri = uris.single()
                    state.kind = missing
                } else {
                    state.uri = null
                    state.completed = true
                }
            } catch (failure: Exception) {
                state.uri = null
                state.problem = (failure as? IdentityFileException)?.problem ?: IdentityFileProblem.INVALID_PAIR
            } finally { selected.forEach { it.close() }; state.busy = false }
        }
    }

    private fun showIdentityFollowUp() {
        val state = identityImport
        if (state.busy || identityDialog != null || !resumed || requestingPermissions || pairingInFlight) return
        if (state.completed) {
            state.completed = false
            refreshMaterialStatus()
            if (importOnly) finish()
            return
        }
        val problem = state.problem
        if (problem != null) {
            state.problem = null
            val message = when (problem) {
                IdentityFileProblem.SELECTION -> R.string.identity_error_selection
                IdentityFileProblem.SAME_KIND -> R.string.identity_error_same_kind
                IdentityFileProblem.INVALID_PAIR -> R.string.identity_error_pair
                IdentityFileProblem.EMPTY_ARCHIVE -> R.string.identity_error_missing
                IdentityFileProblem.DUPLICATE_KEY -> R.string.identity_error_duplicate_key
                IdentityFileProblem.DUPLICATE_CERTIFICATE -> R.string.identity_error_duplicate_certificate
                IdentityFileProblem.ENCRYPTED -> R.string.identity_error_encrypted
                IdentityFileProblem.TOO_LARGE -> R.string.identity_error_size
                IdentityFileProblem.ARCHIVE_LIMIT -> R.string.identity_error_limit
                IdentityFileProblem.BAD_ARCHIVE -> R.string.identity_error_zip
            }
            identityDialog = AlertDialog.Builder(this).setTitle(R.string.direlay_import_error).setMessage(message)
                .setPositiveButton(android.R.string.ok) { _, _ -> if (importOnly) finish() }
                .setOnCancelListener { if (importOnly) finish() }.create()
        } else if (state.uri != null && !state.awaitingPicker) {
            val message = when (state.kind) {
                IdentityKind.KEY -> R.string.identity_follow_key
                IdentityKind.CERTIFICATE -> R.string.identity_follow_certificate
                IdentityKind.UNKNOWN -> R.string.identity_follow_unknown
            }
            fun cancel() { state.uri = null; if (importOnly) finish() }
            identityDialog = AlertDialog.Builder(this).setTitle(R.string.direlay_identity).setMessage(message)
                .setPositiveButton(R.string.identity_choose_other) { _, _ -> openDocument(IDENTITY_CERTIFICATE, R.string.identity_choose_other) }
                .setNegativeButton(android.R.string.cancel) { _, _ -> cancel() }
                .setOnCancelListener { cancel() }.create()
        }
        identityDialog?.let { dialogs.show(it) { identityDialog = null } }
    }

    override fun onActivityResult(requestCode: Int, resultCode: Int, data: Intent?) {
        super.onActivityResult(requestCode, resultCode, data)
        if (requestCode == PAIRING_DIALOG) {
            pairingInFlight = false
            runtime.pairing.dialogResult(PairingVisibilityPolicy.visibleSeconds(resultCode, PairingVisibilityPolicy.dialogSeconds()))
            return
        }
        if (requestCode == VPN_CONSENT) {
            requestOverlayPermission()
            return
        }
        if (resultCode != RESULT_OK) { identityImport.uri = null; identityImport.awaitingPicker = false; if (importOnly) finish(); return }
        if (requestCode == IDENTITY_KEY || requestCode == IDENTITY_CERTIFICATE) {
            val uris = data?.clipData?.let { clip -> (0 until clip.itemCount).map { clip.getItemAt(it).uri } }
                ?: listOfNotNull(data?.data)
            importIdentity(uris)
            return
        }
        val uri = data?.data ?: return
        when (requestCode) {
            PUBLIC_NAME_FILE -> importPublicName(uri)
            TLS_FILE -> {
                val password = EditText(this).apply {
                    inputType = InputType.TYPE_CLASS_TEXT or InputType.TYPE_TEXT_VARIATION_PASSWORD
                    setHint(R.string.direlay_password)
                    filters = arrayOf(android.text.InputFilter.LengthFilter(TlsStore.MAX_PASSWORD_CHARS))
                }
                AlertDialog.Builder(this).setTitle(R.string.direlay_tls_password).setView(password)
                    .setOnCancelListener { password.text.clear(); if (importOnly) finish() }
                    .setNegativeButton(android.R.string.cancel) { _, _ -> password.text.clear(); if (importOnly) finish() }
                    .setPositiveButton(R.string.direlay_import) { _, _ ->
                        val secret = CharArray(password.length()) { password.text[it] }
                        password.text.clear()
                        work(finishAfter = importOnly) {
                            var bytes: ByteArray? = null
                            try {
                                bytes = read(uri, TlsStore.MAX_FILE_BYTES)
                                runtime.tlsStore.importFrom(bytes, secret)
                            } finally { secret.fill('\u0000'); bytes?.fill(0) }
                        }
                    }.let(::showActivityDialog)
            }
        }
    }

    /** Bounded read on the imports worker; problems are listed per key, success names only the provider and the name. */
    private fun importPublicName(uri: Uri) {
        val service = runtime.publicName
        imports.execute {
            val result = try {
                val bytes = try { read(uri, PublicNameFile.MAX_BYTES) } catch (_: IllegalArgumentException) { null }
                if (bytes == null) ConfigParse.Invalid(listOf(ConfigError(ConfigError.FILE, ConfigProblem.TOO_LARGE)))
                else try { service.import(bytes) } finally { bytes.fill(0) }
            } catch (_: Exception) { null }
            handler.post { showPublicNameImport(result) }
        }
    }

    private fun showPublicNameImport(result: ConfigParse?) {
        if (isDestroyed) return
        val builder = AlertDialog.Builder(this)
        when (result) {
            is ConfigParse.Valid -> builder.setTitle(R.string.public_name_title)
                .setMessage(getString(R.string.public_name_imported, result.config.provider?.label.orEmpty(), result.config.name))
            is ConfigParse.Invalid -> builder.setTitle(R.string.direlay_import_error)
                .setMessage(org.openxplay.direlay.ui.publicNameImportProblems(this, result.errors))
            null -> builder.setTitle(R.string.direlay_import_error).setMessage(R.string.direlay_file_unreadable)
        }
        builder.setPositiveButton(android.R.string.ok) { _, _ -> if (importOnly) finish() }
            .setOnCancelListener { if (importOnly) finish() }.let(::showActivityDialog)
    }

    private fun work(finishAfter: Boolean = false, startingSession: Boolean = false, action: () -> Unit) {
        imports.execute {
            try { action(); if (finishAfter) handler.post { if (!isDestroyed) finish() } }
            catch (failure: Exception) {
                if (startingSession) { runtime.status.inputFailed(failure); runtime.startRequest.finish() }
                handler.post {
                    val wasStarting = startingSession
                    refreshPresentation()
                    if (!isDestroyed) AlertDialog.Builder(this)
                        .setTitle(R.string.direlay_import_error)
                        .setMessage(getString(if (wasStarting) R.string.ui_start_failed else R.string.ui_material_failed))
                        .setPositiveButton(android.R.string.ok, null).let(::showActivityDialog)
                }
            }
            updateMaterialStatus()
        }
    }

    private fun refreshMaterialStatus() {
        try { imports.execute { updateMaterialStatus() } }
        catch (_: java.util.concurrent.RejectedExecutionException) { }
    }

    private fun updateMaterialStatus() {
        val identityValue = runtime.identityStore.status()
        handler.post {
            if (!isDestroyed) {
                identityReady = identityValue == MaterialStatus.Installed
                materialChecked = true
                refreshPresentation()
            }
        }
    }

    override fun onSaveInstanceState(outState: Bundle) {
        identityImport.uri?.let { outState.putString("identityUri", it.toString()) }
        outState.putString("identityKind", identityImport.kind.name)
        outState.putBoolean("identityPicker", identityImport.awaitingPicker)
        outState.putBoolean("bootAutoStart", bootAutoStart)
        outState.putBoolean("requestingPermissions", requestingPermissions)
        outState.putBoolean("pairingInFlight", pairingInFlight)
        outState.putString("homeDestination", homeDestination)
        outState.putString("stepNavigation", stepNavigation?.name)
        super.onSaveInstanceState(outState)
    }

    override fun onDestroy() {
        dialogs.close()
        imports.shutdown()
        super.onDestroy()
    }

    companion object {
        /** A separate singleton task keeps bringing up Relay from clearing the CarPlay host. */
        fun intent(context: Context, destination: String? = null, stepAction: StepAction? = null): Intent =
            Intent(context, RelayActivity::class.java)
                .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_REORDER_TO_FRONT or Intent.FLAG_ACTIVITY_SINGLE_TOP)
                .apply {
                    destination?.let { putExtra(EXTRA_HOME_DESTINATION, it) }
                    stepAction?.let { putExtra(EXTRA_STEP_ACTION, it.name) }
                }

        const val EXTRA_HOME_DESTINATION = "org.openxplay.relay.HOME_DESTINATION"
        const val EXTRA_STEP_ACTION = "org.openxplay.relay.STEP_ACTION"
        const val EXTRA_IMPORT_IDENTITY = "org.openxplay.relay.IMPORT_IDENTITY"
        const val EXTRA_IMPORT_TLS = "org.openxplay.relay.IMPORT_TLS"
        const val EXTRA_IMPORT_PUBLIC_NAME = "org.openxplay.relay.IMPORT_PUBLIC_NAME"
        const val EXTRA_AUTO_START = "org.openxplay.relay.AUTO_START"
        private const val PERMISSIONS = 103
        private const val VPN_CONSENT = 104
        private const val PAIRING_PERMISSION = 105
        private const val PAIRING_DIALOG = 106
        private const val IDENTITY_KEY = 100
        private const val IDENTITY_CERTIFICATE = 101
        private const val TLS_FILE = 102
        private const val PUBLIC_NAME_FILE = 107
    }

    override fun onResume() {
        super.onResume()
        presentation.action { runtime.keepAlive.uiVisible() }
        resumed = true
        dialogs.active = !requestingPermissions && !pairingInFlight
        presentation.action { runtime.reloadSettings() }
        // Coming back from system Wi-Fi or Bluetooth settings: show the new network at once.
        lastNetworkRefresh = Long.MIN_VALUE / 2
        materialChecked = false
        refreshNetwork()
        refreshPresentation()
        handler.post(refreshStatus)
        refreshMaterialStatus()
    }

    override fun onPause() {
        resumed = false
        dialogs.active = false
        publishHostFlags()
        content = content?.copy(pending = null)
        handler.removeCallbacks(refreshStatus)
        super.onPause()
    }
}

/** Retains URI references and worker outcomes, never secret bytes, across activity recreation. */
class IdentityImportState : androidx.lifecycle.ViewModel() {
    @Volatile var uri: Uri? = null
    @Volatile var kind = IdentityKind.UNKNOWN
    @Volatile var awaitingPicker = false
    @Volatile var busy = false
    @Volatile var completed = false
    @Volatile var problem: IdentityFileProblem? = null
}
