// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-21
package org.openxplay.direlay.settings

import android.app.Application
import com.shilapi.xcertplay.AirPlayPersistence
import com.shilapi.xcertplay.orchestration.ManualHotspotSecurity
import com.shilapi.xcertplay.orchestration.MfiTarget
import org.openxplay.direlay.R
import org.openxplay.direlay.ui.SettingsFeedback
import org.openxplay.direlay.ui.settingsFeedback
import org.junit.Assert.*
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import org.openxplay.stream.clink.ClinkHevcMode
import org.openxplay.stream.clink.ClinkPictureSize
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [28, 33], manifest = Config.NONE, application = Application::class)
class DeveloperOverridesTest {
    private val context get() = RuntimeEnvironment.getApplication()
    private val relay get() = context.getSharedPreferences("direlay", 0)
    private val chosen = DeveloperOverrides(
        carlinkTrace = true,
        carlinkHevcEnabled = true, carlinkHevc = ClinkHevcMode.entries.first { it != ClinkHevcMode.STREAM_ONLY },
        carlinkPictureSizeEnabled = true, carlinkPictureSize = ClinkPictureSize.MODE,
        physicalWidthEnabled = true, physicalWidthOverrideMm = 225,
        fpsEnabled = true, fps = 55,
        localPreview = true, playCarPlayAudio = true,
        microphoneSourceEnabled = true, microphoneSource = MicrophoneSource.THIS_DEVICE,
        webPortEnabled = true, webPort = 9443,
        carlinkDeviceModelEnabled = true, carlinkDeviceModel = "Test relay",
    )

    @Before fun clear() {
        for (name in listOf("direlay", "diplay", "xcertplay_airplay"))
            context.getSharedPreferences(name, 0).edit().clear().commit()
    }

    @Test fun effectiveSettingsRequireBothGlobalSwitchesAndPreserveEveryUngovernedField() {
        RelaySettingsStore(context).use { store ->
            val base = store.load().copy(
                connectionMode = ConnectionMode.USB, mfiTarget = MfiTarget.I2C, i2cPath = "/dev/i2c-3",
                remoteServer = "https://mfi.example.test", remoteToken = "token",
                display = DisplayProfile(1280, 720, 160), displayMode = DisplayMode.FIXED,
                autoStartOnBoot = true, autoStartWhenReady = false,
                carlinkOutput = true, carlinkDeviceName = "My car", bothOutputs = true,
                vehicleInformation = true, carlinkAutoReconnect = false, iphoneAddress = "phone",
                wirelessNetwork = WirelessNetworkSettings(channel = 6, security = ManualHotspotSecurity.WPA2),
                plugins = true, pluginsEnabled = listOf("test"), viewAreas = false, car = CarSelection("xpeng", model = "g6-2023"),
                videoCodec = VideoCodecSetting.H264, videoStats = VideoStatsWindow.SECONDS_5,
                carplayResolution = CarPlayResolution.P720, carplayScreenWidth = CarPlayScreenWidth.MM300,
                carBluetoothAudio = true,
                fps = 60, localPreview = true, playCarPlayAudio = true, webPort = 10443,
                carlinkDeviceModel = "Base model", microphoneSource = MicrophoneSource.THIS_DEVICE,
                carlinkTrace = true, carlinkHevc = chosen.carlinkHevc, carlinkPictureSize = ClinkPictureSize.MODE,
            )
            val defaults = base.copy(carlinkTrace = false, carlinkHevc = ClinkHevcMode.STREAM_ONLY,
                carlinkPictureSize = ClinkPictureSize.WINDOW, fps = 30, localPreview = false,
                playCarPlayAudio = false, microphoneSource = MicrophoneSource.HEAD_UNIT, webPort = null,
                carlinkDeviceModel = "Android relay")
            val enabled = base.copy(fps = 55, webPort = 9443, carlinkDeviceModel = "Test relay")
            for (mode in listOf(false, true)) for (master in listOf(false, true)) {
                val overrides = chosen.copy(active = master)
                assertEquals(if (mode && master) enabled else defaults, overrides.effectiveSettings(base, mode))
                assertEquals(if (mode && master) 225 else 0, overrides.effectivePhysicalWidth(mode))
                for (sample in listOf(false, true))
                    assertEquals(sample && mode && master, overrides.effectiveSampleVehicleData(sample, mode))
            }
            val items = listOf(
                chosen.copy(carlinkTrace = false) to enabled.copy(carlinkTrace = false),
                chosen.copy(carlinkHevcEnabled = false) to enabled.copy(carlinkHevc = ClinkHevcMode.STREAM_ONLY),
                chosen.copy(carlinkPictureSizeEnabled = false) to enabled.copy(carlinkPictureSize = ClinkPictureSize.WINDOW),
                chosen.copy(fpsEnabled = false) to enabled.copy(fps = 30),
                chosen.copy(localPreview = false) to enabled.copy(localPreview = false),
                chosen.copy(playCarPlayAudio = false) to enabled.copy(playCarPlayAudio = false),
                chosen.copy(microphoneSourceEnabled = false) to enabled.copy(microphoneSource = MicrophoneSource.HEAD_UNIT),
                chosen.copy(webPortEnabled = false) to enabled.copy(webPort = null),
                chosen.copy(carlinkDeviceModelEnabled = false) to enabled.copy(carlinkDeviceModel = "Android relay"),
                chosen.copy(physicalWidthEnabled = false) to enabled,
            )
            for ((overrides, expected) in items) {
                assertEquals(expected, overrides.effectiveSettings(base, true))
                assertEquals(if (overrides.physicalWidthEnabled) 225 else 0, overrides.effectivePhysicalWidth(true))
                assertEquals(defaults, overrides.effectiveSettings(base, false))
                assertEquals(defaults, overrides.copy(active = false).effectiveSettings(base, true))
            }
            assertNull(chosen.copy(webPort = null).effectiveSettings(base, true).webPort)
        }
    }

    @Test fun migrationPreservesLegacyValuesAndMakesHiddenOverridesVisible() {
        relay.edit().putBoolean("developerMode", false)
            .putBoolean("carlinkTrace", true).putString("carlinkHevc", chosen.carlinkHevc.name)
            .putString("carlinkPictureSize", chosen.carlinkPictureSize.name).putInt("devPhysicalWidthMm", 225)
            .putBoolean("localPreview", true).putBoolean("playCarPlayAudio", true)
            .putString("microphoneSource", MicrophoneSource.THIS_DEVICE.name)
            .putInt("webPort", 9443).putString("carlinkDeviceModel", "Test relay").commit()
        AirPlayPersistence.saveFps(context, 55)
        RelaySettingsStore(context).use { store ->
            val settings = store.awaitSnapshot()
            assertTrue(store.developerMode)
            assertEquals(chosen, store.developerOverrides.value)
            assertEquals(55, settings.fps)
            assertEquals(9443, settings.webPort)
            assertEquals(225, store.physicalWidthOverrideMm)
            assertTrue(settings.carlinkTrace && settings.localPreview && settings.playCarPlayAudio)
            assertEquals(chosen.carlinkHevc, settings.carlinkHevc)
            assertEquals(chosen.carlinkPictureSize, settings.carlinkPictureSize)
            assertEquals(MicrophoneSource.THIS_DEVICE, settings.microphoneSource)
            assertEquals("Test relay", settings.carlinkDeviceModel)
            store.developerMode = false
        }
        RelaySettingsStore(context).use { store ->
            assertEquals(30, store.load().fps)
            assertFalse(store.developerMode)
            assertEquals(chosen, store.developerOverrides.value)
        }
    }

    @Test fun defaultMigrationLeavesDeveloperModeOffAndValueSwitchesOff() {
        assertEquals(30, AirPlayPersistence.loadFps(context))
        relay.edit().putBoolean("bothOutputs", true).putBoolean("vehicleInformation", true).commit()
        RelaySettingsStore(context).use { store ->
            val settings = store.load()
            assertFalse(store.developerMode)
            assertEquals(DeveloperOverrides(), store.developerOverrides.value)
            assertTrue(settings.bothOutputs && settings.vehicleInformation)
        }
    }

    @Test fun eachLegacyOverrideAloneEnablesDeveloperMode() {
        val changes: List<() -> Unit> = listOf(
            { relay.edit().putBoolean("carlinkTrace", true).commit(); Unit },
            { relay.edit().putString("carlinkHevc", chosen.carlinkHevc.name).commit(); Unit },
            { relay.edit().putString("carlinkPictureSize", ClinkPictureSize.MODE.name).commit(); Unit },
            { relay.edit().putInt("devPhysicalWidthMm", 200).commit(); Unit },
            { AirPlayPersistence.saveFps(context, 60) },
            { relay.edit().putBoolean("localPreview", true).commit(); Unit },
            { relay.edit().putBoolean("playCarPlayAudio", true).commit(); Unit },
            { relay.edit().putString("microphoneSource", MicrophoneSource.THIS_DEVICE.name).commit(); Unit },
            { relay.edit().putInt("webPort", 9443).commit(); Unit },
            { relay.edit().putString("carlinkDeviceModel", "Test relay").commit(); Unit },
        )
        for (change in changes) {
            clear()
            change()
            RelaySettingsStore(context).use { store ->
                store.load()
                assertTrue(store.developerMode)
                assertTrue(store.developerOverrides.value.active)
            }
        }
    }

    @Test fun masterWritesEffectiveDefaultsAndRestoresRememberedValuesAfterReopening() {
        RelaySettingsStore(context).use { store ->
            store.developerMode = true
            store.saveDeveloperOverrides(chosen)
            store.save(store.load().copy(display = DisplayProfile(2400, 1200, 300), displayMode = DisplayMode.FIXED))
            assertEquals(225, AirPlayPersistence.loadWidthPhysicalMm(context))
            store.saveDeveloperOverrides(chosen.copy(active = false))
            val effective = checkNotNull(store.snapshot.value)
            assertEquals(DeveloperOverrides().effectiveSettings(effective, true), effective)
            assertEquals(effective, store.load())
            assertEquals(effective, store.awaitSnapshot())
            assertEquals(30, AirPlayPersistence.loadFps(context))
            assertFalse(relay.getBoolean("carlinkTrace", true))
            assertFalse(relay.getBoolean("localPreview", true))
            assertFalse(relay.getBoolean("playCarPlayAudio", true))
            assertEquals(ClinkHevcMode.STREAM_ONLY.name, relay.getString("carlinkHevc", null))
            assertEquals(ClinkPictureSize.WINDOW.name, relay.getString("carlinkPictureSize", null))
            assertEquals(MicrophoneSource.HEAD_UNIT.name, relay.getString("microphoneSource", null))
            assertEquals(0, relay.getInt("webPort", -1))
            assertEquals("Android relay", relay.getString("carlinkDeviceModel", null))
            assertEquals(0, relay.getInt("devPhysicalWidthMm", -1))
            assertEquals(250, AirPlayPersistence.loadWidthPhysicalMm(context))
            assertEquals(55, relay.getInt("developer.fps", 0))
            assertEquals(225, relay.getInt("developer.physicalWidthMm", 0))
            assertEquals(chosen.copy(active = false), store.developerOverrides.value)
        }
        RelaySettingsStore(context).use { store ->
            val defaults = store.awaitSnapshot()
            assertEquals(chosen.copy(active = false), store.developerOverrides.value)
            store.saveDeveloperOverrides(store.developerOverrides.value.copy(active = true))
            assertEquals(chosen.effectiveSettings(defaults, true), store.snapshot.value)
            assertEquals(55, AirPlayPersistence.loadFps(context))
            assertEquals(225, AirPlayPersistence.loadWidthPhysicalMm(context))
            assertEquals(chosen, store.developerOverrides.value)
        }
    }

    @Test fun developerModeReturnsToDefaultsAndRestoresTheMasterAndItemChoices() {
        RelaySettingsStore(context).use { store ->
            store.developerMode = true
            store.saveDeveloperOverrides(chosen.copy(fpsEnabled = false))
            val enabled = store.load()
            store.developerMode = false
            assertEquals(DeveloperOverrides().effectiveSettings(enabled, true), store.snapshot.value)
            assertEquals(0, store.physicalWidthOverrideMm)
            assertEquals(30, AirPlayPersistence.loadFps(context))
            assertEquals(chosen.copy(fpsEnabled = false), store.developerOverrides.value)
            store.developerMode = true
            assertEquals(enabled, store.snapshot.value)
            assertEquals(225, store.physicalWidthOverrideMm)
            store.saveDeveloperOverrides(chosen.copy(active = false))
            store.developerMode = false
            store.developerMode = true
            assertFalse(store.developerOverrides.value.active)
            assertEquals(30, store.load().fps)
        }
    }

    @Test fun valueItemSwitchesKeepTheirValuesWhileOff() {
        RelaySettingsStore(context).use { store ->
            store.developerMode = true
            store.saveDeveloperOverrides(chosen)
            val disabled = chosen.copy(carlinkHevcEnabled = false, carlinkPictureSizeEnabled = false,
                physicalWidthEnabled = false, fpsEnabled = false, microphoneSourceEnabled = false,
                webPortEnabled = false, carlinkDeviceModelEnabled = false)
            store.saveDeveloperOverrides(disabled)
            assertEquals(disabled, store.developerOverrides.value)
            assertEquals(30, store.load().fps)
            assertNull(store.load().webPort)
            assertEquals(0, store.physicalWidthOverrideMm)
            store.saveDeveloperOverrides(chosen)
            assertEquals(55, store.load().fps)
            assertEquals(9443, store.load().webPort)
            assertEquals(225, store.physicalWidthOverrideMm)
        }
    }

    @Test fun ordinarySavesIgnoreInactiveOverridesButStillSaveOtherSettings() {
        for (gate in 0..2) RelaySettingsStore(context).use { store ->
            clear()
            store.developerMode = gate != 0
            val choices = when (gate) {
                0 -> chosen
                1 -> chosen.copy(active = false)
                else -> chosen.copy(carlinkTrace = false, localPreview = false, playCarPlayAudio = false,
                    carlinkHevcEnabled = false, carlinkPictureSizeEnabled = false, fpsEnabled = false,
                    physicalWidthEnabled = false, microphoneSourceEnabled = false, webPortEnabled = false,
                    carlinkDeviceModelEnabled = false)
            }
            store.saveDeveloperOverrides(choices)
            val effective = store.load()
            store.save(effective.copy(fps = 60, webPort = 10443, carlinkDeviceModel = "Incoming model",
                carlinkHevc = chosen.carlinkHevc, carlinkPictureSize = chosen.carlinkPictureSize,
                microphoneSource = MicrophoneSource.THIS_DEVICE, carlinkTrace = true,
                localPreview = true, playCarPlayAudio = true, autoStartOnBoot = true))
            assertEquals(choices, store.developerOverrides.value)
            assertEquals(effective.copy(autoStartOnBoot = true), store.load())
            assertEquals(30, AirPlayPersistence.loadFps(context))
        }
    }

    @Test fun ordinarySavesUpdateRememberedValuesOfActiveOverrides() {
        RelaySettingsStore(context).use { store ->
            store.developerMode = true
            store.saveDeveloperOverrides(chosen)
            val incoming = store.load().copy(fps = 60, webPort = null, carlinkDeviceModel = "Changed model",
                carlinkHevc = ClinkHevcMode.STREAM_ONLY, carlinkPictureSize = ClinkPictureSize.WINDOW,
                microphoneSource = MicrophoneSource.HEAD_UNIT, carlinkTrace = false, localPreview = false,
                playCarPlayAudio = false)
            store.save(incoming)
            val remembered = chosen.copy(fps = 60, webPort = null, carlinkDeviceModel = "Changed model",
                carlinkHevc = ClinkHevcMode.STREAM_ONLY, carlinkPictureSize = ClinkPictureSize.WINDOW,
                microphoneSource = MicrophoneSource.HEAD_UNIT, carlinkTrace = false, localPreview = false,
                playCarPlayAudio = false)
            assertEquals(remembered, store.developerOverrides.value)
            assertEquals(incoming, store.snapshot.value)
            store.saveDeveloperOverrides(remembered.copy(active = false))
            store.saveDeveloperOverrides(remembered)
            assertEquals(incoming, store.load())
        }
    }

    @Test fun feedbackUsesEffectiveValuesAndIncludesPhysicalWidthChanges() {
        RelaySettingsStore(context).use { store ->
            val base = store.load()
            val options = DeveloperOverrides(fpsEnabled = true, fps = 55)
            val enabled = options.effectiveSettings(base, true)
            val disabled = options.effectiveSettings(base, false)
            assertEquals(SettingsFeedback(R.string.settings_next_start, R.string.settings_restart),
                settingsFeedback(enabled, disabled, false, true))
            assertNull(settingsFeedback(disabled, options.copy(fps = 60).effectiveSettings(base, false), false, true))
            assertEquals(SettingsFeedback(R.string.settings_next_start, R.string.settings_restart),
                settingsFeedback(base, base, false, true, 225, 0))
            assertNull(settingsFeedback(base, base, false, true, 0, 0))
            val picture = DeveloperOverrides(carlinkPictureSizeEnabled = true, carlinkPictureSize = ClinkPictureSize.MODE)
            assertEquals(SettingsFeedback(R.string.settings_reconnect, R.string.vehicle_reconnect_action),
                settingsFeedback(picture.effectiveSettings(base, true), picture.effectiveSettings(base, false), true, true))
        }
    }
}
