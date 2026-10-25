// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-21
package org.openxplay.direlay.settings

import org.openxplay.stream.core.FrameRatePreference
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
        localPreview = true, playCarPlayAudio = true,
        microphoneSourceEnabled = true, microphoneSource = MicrophoneSource.THIS_DEVICE,
        webPortEnabled = true, webPort = 9443,
        carlinkDeviceModelEnabled = true, carlinkDeviceModel = "Test relay",
    )

    @Before fun clear() {
        for (name in listOf("direlay", "diplay", "xcertplay_airplay"))
            context.getSharedPreferences(name, 0).edit().clear().commit()
    }

    @Test fun effectiveSettingsUseTheApplySwitchAndPreserveEveryUngovernedField() {
        RelaySettingsStore(context).use { store ->
            val base = store.load().copy(
                connectionMode = ConnectionMode.USB, mfiTarget = MfiTarget.I2C, i2cPath = "/dev/i2c-3",
                remoteServer = "https://mfi.example.test", remoteToken = "token",
                display = DisplayProfile(1280, 720, 160), displayMode = DisplayMode.FIXED,
                autoStartOnBoot = true, autoStartWhenReady = false,
                carlinkOutput = true, carlinkDeviceName = "My car", bothOutputs = true,
                vehicleInformation = true, carlinkAutoReconnect = false, iphoneAddress = "phone",
                wirelessNetwork = WirelessNetworkSettings(channel = 6, security = ManualHotspotSecurity.WPA2),
                plugins = true, pluginsEnabled = listOf("test"), viewAreas = false, car = CarSelection("xpeng", model = "g6", year = "g6-2023"),
                videoCodec = VideoCodecSetting.H264, videoStats = VideoStatsWindow.SECONDS_5,
                carplayResolution = CarPlayResolution.P720, carplayScreenWidth = CarPlayScreenWidth.MM300,
                carBluetoothAudio = true,
                frameRate = FrameRatePreference.FPS_60, localPreview = true, playCarPlayAudio = true, webPort = 10443,
                carlinkDeviceModel = "Base model", microphoneSource = MicrophoneSource.THIS_DEVICE,
                carlinkTrace = true, carlinkHevc = chosen.carlinkHevc, carlinkPictureSize = ClinkPictureSize.MODE,
            )
            val defaults = base.copy(carlinkTrace = false, carlinkHevc = ClinkHevcMode.STREAM_ONLY,
                carlinkPictureSize = ClinkPictureSize.WINDOW, localPreview = false,
                playCarPlayAudio = false, microphoneSource = MicrophoneSource.HEAD_UNIT, webPort = null,
                carlinkDeviceModel = "Android relay")
            val enabled = base.copy(webPort = 9443, carlinkDeviceModel = "Test relay")
            for (master in listOf(false, true)) {
                val overrides = chosen.copy(active = master)
                assertEquals(if (master) enabled else defaults, overrides.effectiveSettings(base))
                assertEquals(if (master) 225 else 0, overrides.effectivePhysicalWidth())
                for (sample in listOf(false, true))
                    assertEquals(sample && master, overrides.effectiveSampleVehicleData(sample))
            }
            val items = listOf(
                chosen.copy(carlinkTrace = false) to enabled.copy(carlinkTrace = false),
                chosen.copy(carlinkHevcEnabled = false) to enabled.copy(carlinkHevc = ClinkHevcMode.STREAM_ONLY),
                chosen.copy(carlinkPictureSizeEnabled = false) to enabled.copy(carlinkPictureSize = ClinkPictureSize.WINDOW),
                chosen.copy(localPreview = false) to enabled.copy(localPreview = false),
                chosen.copy(playCarPlayAudio = false) to enabled.copy(playCarPlayAudio = false),
                chosen.copy(microphoneSourceEnabled = false) to enabled.copy(microphoneSource = MicrophoneSource.HEAD_UNIT),
                chosen.copy(webPortEnabled = false) to enabled.copy(webPort = null),
                chosen.copy(carlinkDeviceModelEnabled = false) to enabled.copy(carlinkDeviceModel = "Android relay"),
                chosen.copy(physicalWidthEnabled = false) to enabled,
            )
            for ((overrides, expected) in items) {
                assertEquals(expected, overrides.effectiveSettings(base))
                assertEquals(if (overrides.physicalWidthEnabled) 225 else 0, overrides.effectivePhysicalWidth())
                assertEquals(defaults, overrides.copy(active = false).effectiveSettings(base))
            }
            assertNull(chosen.copy(webPort = null).effectiveSettings(base).webPort)
        }
    }

    @Test fun migrationPreservesLegacyValuesWithoutReadingOrWritingDeveloperMode() {
        relay.edit().putBoolean("developerMode", false)
            .putBoolean("carlinkTrace", true).putString("carlinkHevc", chosen.carlinkHevc.name)
            .putString("carlinkPictureSize", chosen.carlinkPictureSize.name).putInt("devPhysicalWidthMm", 225)
            .putBoolean("localPreview", true).putBoolean("playCarPlayAudio", true)
            .putString("microphoneSource", MicrophoneSource.THIS_DEVICE.name)
            .putInt("webPort", 9443).putString("carlinkDeviceModel", "Test relay").commit()
        AirPlayPersistence.saveFps(context, 55)
        RelaySettingsStore(context).use { store ->
            val settings = store.awaitSnapshot()
            assertFalse(relay.getBoolean("developerMode", false))
            assertEquals(chosen, store.developerOverrides.value)
            assertEquals(FrameRatePreference.AUTO, settings.frameRate)
            assertEquals(9443, settings.webPort)
            assertEquals(225, store.physicalWidthOverrideMm)
            assertTrue(settings.carlinkTrace && settings.localPreview && settings.playCarPlayAudio)
            assertEquals(chosen.carlinkHevc, settings.carlinkHevc)
            assertEquals(chosen.carlinkPictureSize, settings.carlinkPictureSize)
            assertEquals(MicrophoneSource.THIS_DEVICE, settings.microphoneSource)
            assertEquals("Test relay", settings.carlinkDeviceModel)
            store.saveDeveloperOverrides(chosen.copy(active = false))
        }
        RelaySettingsStore(context).use { store ->
            assertEquals(FrameRatePreference.AUTO, store.load().frameRate)
            assertFalse(relay.getBoolean("developerMode", false))
            assertEquals(chosen.copy(active = false), store.developerOverrides.value)
        }
    }

    @Test fun defaultMigrationLeavesTheLegacyModeKeyUntouchedAndValueSwitchesOff() {
        assertEquals(30, AirPlayPersistence.loadFps(context))
        relay.edit().putBoolean("bothOutputs", true).putBoolean("vehicleInformation", true).commit()
        RelaySettingsStore(context).use { store ->
            val settings = store.load()
            assertFalse(relay.getBoolean("developerMode", false))
            assertEquals(DeveloperOverrides(), store.developerOverrides.value)
            assertTrue(settings.bothOutputs && settings.vehicleInformation)
        }
    }

    @Test fun freshStorePartialEditsPreserveEveryRememberedOverrideAndSwitch() {
        val disabledItems = chosen.copy(carlinkHevcEnabled = false, carlinkPictureSizeEnabled = false,
            physicalWidthEnabled = false, microphoneSourceEnabled = false,
            webPortEnabled = false, carlinkDeviceModelEnabled = false)
        for (initial in listOf(chosen, chosen.copy(active = false), disabledItems)) {
            clear()
            relay.edit().putBoolean("developerMode", false).commit()
            RelaySettingsStore(context).use { it.saveDeveloperOverrides(initial) }
            val expected = initial.copy(playCarPlayAudio = false)
            RelaySettingsStore(context).use { store ->
                // No load/awaitSnapshot: opening a store and editing one choice must retain the others.
                val observed = store.developerOverrides
                assertEquals(initial, observed.value)
                store.saveDeveloperOverrides(observed.value.copy(playCarPlayAudio = false))
                assertEquals(expected, observed.value)
            }
            RelaySettingsStore(context).use { store ->
                assertEquals(expected, store.developerOverrides.value)
                val settings = store.load()
                assertEquals(expected.effectiveSettings(settings), settings)
                assertEquals(expected.effectivePhysicalWidth(), store.physicalWidthOverrideMm)
                assertFalse(relay.getBoolean("developerMode", true))
            }
        }
    }

    @Test fun freshStoreOverrideReadMigratesLegacyChoicesBeforeAPartialEdit() {
        relay.edit().putBoolean("developerMode", false)
            .putString("microphoneSource", MicrophoneSource.THIS_DEVICE.name)
            .putInt("devPhysicalWidthMm", 225).commit()
        AirPlayPersistence.saveFps(context, 55)
        val migrated = DeveloperOverrides(microphoneSourceEnabled = true,
            microphoneSource = MicrophoneSource.THIS_DEVICE, physicalWidthEnabled = true,
            physicalWidthOverrideMm = 225)
        RelaySettingsStore(context).use { store ->
            assertEquals(migrated, store.developerOverrides.value)
            store.saveDeveloperOverrides(store.developerOverrides.value.copy(playCarPlayAudio = true))
        }
        RelaySettingsStore(context).use { store ->
            assertEquals(migrated.copy(playCarPlayAudio = true), store.developerOverrides.value)
            val settings = store.load()
            assertEquals(MicrophoneSource.THIS_DEVICE, settings.microphoneSource)
            assertEquals(FrameRatePreference.AUTO, settings.frameRate)
            assertEquals(225, store.physicalWidthOverrideMm)
            assertTrue(settings.playCarPlayAudio)
            assertFalse(relay.getBoolean("developerMode", true))
        }
    }

    @Test fun eachLegacyOverrideAloneRemainsActiveWithoutDeveloperMode() {
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
                assertFalse(relay.getBoolean("developerMode", false))
                assertTrue(store.developerOverrides.value.active)
            }
        }
    }

    @Test fun masterWritesEffectiveDefaultsAndRestoresRememberedValuesAfterReopening() {
        RelaySettingsStore(context).use { store ->
            store.saveDeveloperOverrides(chosen)
            store.save(store.load().copy(display = DisplayProfile(2400, 1200, 300), displayMode = DisplayMode.FIXED))
            assertEquals(225, AirPlayPersistence.loadWidthPhysicalMm(context))
            store.saveDeveloperOverrides(chosen.copy(active = false))
            val effective = checkNotNull(store.snapshot.value)
            assertEquals(DeveloperOverrides().effectiveSettings(effective), effective)
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
            assertFalse(relay.contains("developer.fps"))
            assertEquals(225, relay.getInt("developer.physicalWidthMm", 0))
            assertEquals(chosen.copy(active = false), store.developerOverrides.value)
        }
        RelaySettingsStore(context).use { store ->
            val defaults = store.awaitSnapshot()
            assertEquals(chosen.copy(active = false), store.developerOverrides.value)
            store.saveDeveloperOverrides(store.developerOverrides.value.copy(active = true))
            assertEquals(chosen.effectiveSettings(defaults), store.snapshot.value)
            assertEquals(30, AirPlayPersistence.loadFps(context))
            assertEquals(225, AirPlayPersistence.loadWidthPhysicalMm(context))
            assertEquals(chosen, store.developerOverrides.value)
        }
    }

    @Test fun legacyDeveloperModeKeyDoesNotGateOrChangeOverrideChoices() {
        RelaySettingsStore(context).use { store ->
            store.saveDeveloperOverrides(chosen)
            val enabled = store.load()
            for (legacyMode in listOf(false, true, false)) {
                relay.edit().putBoolean("developerMode", legacyMode).commit()
                assertEquals(enabled, store.load())
                assertEquals(chosen, store.developerOverrides.value)
                assertEquals(225, store.physicalWidthOverrideMm)
                assertEquals(legacyMode, relay.getBoolean("developerMode", !legacyMode))
            }
            store.saveDeveloperOverrides(chosen.copy(active = false))
            assertEquals(FrameRatePreference.AUTO, store.load().frameRate)
            assertFalse(store.developerOverrides.value.active)
        }
    }

    @Test fun valueItemSwitchesKeepTheirValuesWhileOff() {
        RelaySettingsStore(context).use { store ->
            store.saveDeveloperOverrides(chosen)
            val disabled = chosen.copy(carlinkHevcEnabled = false, carlinkPictureSizeEnabled = false,
                physicalWidthEnabled = false, microphoneSourceEnabled = false,
                webPortEnabled = false, carlinkDeviceModelEnabled = false)
            store.saveDeveloperOverrides(disabled)
            assertEquals(disabled, store.developerOverrides.value)
            assertEquals(FrameRatePreference.AUTO, store.load().frameRate)
            assertNull(store.load().webPort)
            assertEquals(0, store.physicalWidthOverrideMm)
            store.saveDeveloperOverrides(chosen)
            assertEquals(FrameRatePreference.AUTO, store.load().frameRate)
            assertEquals(9443, store.load().webPort)
            assertEquals(225, store.physicalWidthOverrideMm)
        }
    }

    @Test fun ordinarySavesIgnoreInactiveOverridesButStillSaveOtherSettings() {
        for (gate in 0..1) RelaySettingsStore(context).use { store ->
            clear()
            val choices = when (gate) {
                0 -> chosen.copy(active = false)
                else -> chosen.copy(carlinkTrace = false, localPreview = false, playCarPlayAudio = false,
                    carlinkHevcEnabled = false, carlinkPictureSizeEnabled = false,
                    physicalWidthEnabled = false, microphoneSourceEnabled = false, webPortEnabled = false,
                    carlinkDeviceModelEnabled = false)
            }
            store.saveDeveloperOverrides(choices)
            val effective = store.load()
            store.save(effective.copy(frameRate = FrameRatePreference.FPS_60, webPort = 10443, carlinkDeviceModel = "Incoming model",
                carlinkHevc = chosen.carlinkHevc, carlinkPictureSize = chosen.carlinkPictureSize,
                microphoneSource = MicrophoneSource.THIS_DEVICE, carlinkTrace = true,
                localPreview = true, playCarPlayAudio = true, autoStartOnBoot = true))
            assertEquals(choices, store.developerOverrides.value)
            assertEquals(effective.copy(autoStartOnBoot = true, frameRate = FrameRatePreference.FPS_60), store.load())
            assertEquals(30, AirPlayPersistence.loadFps(context))
        }
    }

    @Test fun ordinarySavesUpdateRememberedValuesOfActiveOverrides() {
        RelaySettingsStore(context).use { store ->
            store.saveDeveloperOverrides(chosen)
            val incoming = store.load().copy(frameRate = FrameRatePreference.FPS_60, webPort = null, carlinkDeviceModel = "Changed model",
                carlinkHevc = ClinkHevcMode.STREAM_ONLY, carlinkPictureSize = ClinkPictureSize.WINDOW,
                microphoneSource = MicrophoneSource.HEAD_UNIT, carlinkTrace = false, localPreview = false,
                playCarPlayAudio = false)
            store.save(incoming)
            val remembered = chosen.copy(webPort = null, carlinkDeviceModel = "Changed model",
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

    @Test fun editorPublishesNormalSettingsAndWindowFitChoicesTogetherAndRejectsInvalidSettings() {
        RelaySettingsStore(context).use { store ->
            val base = store.awaitSnapshot()
            val next = base.copy(autoStartOnBoot = true, carlinkOutput = true)
            store.save(next, chosen)
            assertEquals(chosen.effectiveSettings(next), store.snapshot.value)
            assertEquals(chosen, store.developerOverrides.value)
            assertEquals(ClinkPictureSize.MODE.name, relay.getString("carlinkPictureSize", null))
            assertEquals(225, store.physicalWidthOverrideMm)
            val invalid = next.copy(carlinkDeviceName = "")
            assertTrue(runCatching { store.save(invalid, chosen.copy(active = false)) }.isFailure)
            assertEquals(chosen.effectiveSettings(next), store.load())
            assertEquals(chosen, store.developerOverrides.value)
        }
    }

    @Test fun feedbackUsesEffectiveValuesAndIncludesPhysicalWidthChanges() {
        RelaySettingsStore(context).use { store ->
            val base = store.load()
            assertNull(settingsFeedback(base, base.copy(frameRate = FrameRatePreference.FPS_55), false, true))
            assertEquals(SettingsFeedback(R.string.settings_next_start, R.string.settings_restart),
                settingsFeedback(base, base, false, true, 225, 0))
            assertNull(settingsFeedback(base, base, false, true, 0, 0))
            val picture = DeveloperOverrides(carlinkPictureSizeEnabled = true, carlinkPictureSize = ClinkPictureSize.MODE)
            assertEquals(SettingsFeedback(R.string.settings_reconnect, R.string.vehicle_reconnect_action),
                settingsFeedback(picture.effectiveSettings(base), picture.copy(active = false).effectiveSettings(base), true, true))
        }
    }
}
