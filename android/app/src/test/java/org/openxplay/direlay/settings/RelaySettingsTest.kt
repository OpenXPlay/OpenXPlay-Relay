// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-14
package org.openxplay.direlay.settings

import org.openxplay.stream.core.FrameRatePreference
import android.app.Application
import android.os.Looper
import org.robolectric.Shadows.shadowOf
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import kotlinx.coroutines.flow.filterNotNull
import kotlinx.coroutines.flow.first
import com.shilapi.xcertplay.AirPlayPersistence
import com.shilapi.xcertplay.airplay.AirPlayPhysicalSizeBasis
import com.shilapi.xcertplay.orchestration.MfiTarget
import org.junit.Assert.*
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [28, 33], manifest = Config.NONE, application = Application::class)
class RelaySettingsTest {
    @Test fun frameRateMigratesOnlyAnEnabledValidDeveloperChoiceAndDropsBothKeys() {
        store.use { store ->
            val prefs = context.getSharedPreferences("direlay", 0)
            for (enabled in listOf(false, true)) for (fps in listOf(29, 30, 35, 40, 45, 50, 55, 60, 61)) {
                prefs.edit().remove("frameRate").putBoolean("developer.fps.enabled", enabled).putInt("developer.fps", fps).commit()
                val expected = if (enabled) FrameRatePreference.entries.firstOrNull { it.name == "FPS_$fps" } ?: FrameRatePreference.AUTO
                    else FrameRatePreference.AUTO
                assertEquals(expected, store.load().frameRate)
                assertEquals(expected.name, prefs.getString("frameRate", null))
                assertFalse(prefs.contains("developer.fps.enabled"))
                assertFalse(prefs.contains("developer.fps"))
                assertEquals(expected, RelaySettingsStore(context).use { it.load().frameRate })
            }
            for (stored in listOf("AUTO", "FPS_45", "future")) {
                prefs.edit().putString("frameRate", stored).putBoolean("developer.fps.enabled", true).putInt("developer.fps", 55).commit()
                val expected = FrameRatePreference.entries.firstOrNull { it.name == stored } ?: FrameRatePreference.AUTO
                assertEquals(expected, store.load().frameRate)
                assertFalse(prefs.contains("developer.fps"))
            }
        }
    }

    @Test fun frameRateIsAUserSettingSharedWithTheDisplayPortWithoutOverwritingSessionFps() {
        store.use { store ->
            val notifications = mutableListOf<org.openxplay.stream.web.DisplaySettings>()
            store.awaitSnapshot()
            AirPlayPersistence.saveFps(context, 40)
            store.listen { notifications.add(it) }.use {
                for (frameRate in FrameRatePreference.entries) {
                    store.save(store.load().copy(frameRate = frameRate))
                    assertEquals(frameRate, store.current().frameRate)
                    assertEquals(frameRate, store.snapshot.value!!.frameRate)
                    assertEquals(frameRate, store.load().frameRate)
                    assertEquals(40, AirPlayPersistence.loadFps(context))
                }
                store.set(store.current().copy(frameRate = FrameRatePreference.FPS_45), org.openxplay.stream.core.Origin("web", "primary"))
                assertEquals(FrameRatePreference.FPS_45, store.snapshot.value!!.frameRate)
                assertEquals(store.current(), notifications.last())
                assertEquals(FrameRatePreference.entries.size, notifications.size)
                store.saveDeveloperOverrides(store.developerOverrides.value.copy(active = false))
                assertEquals(FrameRatePreference.FPS_45, store.load().frameRate)
                assertEquals(40, AirPlayPersistence.loadFps(context))
            }
        }
    }

    private val context get() = RuntimeEnvironment.getApplication()
    private val store get() = RelaySettingsStore(context)

    @Test fun highPerformanceDefaultsOnAndRoundTrips() {
        store.use { store ->
            val initial = store.load()
            assertTrue(initial.highPerformance)
            for (highPerformance in listOf(false, true)) {
                val chosen = initial.copy(highPerformance = highPerformance)
                store.save(chosen)
                RelaySettingsStore(context).use { assertEquals(chosen, it.load()) }
                val prefs = context.getSharedPreferences("direlay", 0)
                assertEquals(highPerformance, prefs.getBoolean("highPerformance", !highPerformance))
            }
        }
    }

    @Test fun wirelessPictureDefaultsAndParkedCustomSizeRoundTripAndInvalidSizesAreRejected() {
        store.use { store ->
            val initial = store.load()
            assertEquals(WirelessPictureMode.AUTO, initial.wirelessPicture)
            assertEquals(WirelessPictureSize(1280, 720), initial.wirelessPictureSize)
            assertFalse(initial.miracastStretch)
            for (mode in WirelessPictureMode.entries) for (stretch in listOf(false, true))
                for (size in listOf(WirelessPictureSize(1920, 720), WirelessPictureSize(720, 1280))) {
                    val chosen = initial.copy(wirelessPicture = mode, wirelessPictureSize = size, miracastStretch = stretch)
                    store.save(chosen)
                    RelaySettingsStore(context).use { assertEquals(chosen, it.load()) }
                    val prefs = context.getSharedPreferences("direlay", 0)
                    assertEquals(mode.name, prefs.getString("wirelessPicture", null))
                    assertEquals(size.width, prefs.getInt("wirelessPictureWidth", 0))
                    assertEquals(size.height, prefs.getInt("wirelessPictureHeight", 0))
                    assertEquals(stretch, prefs.getBoolean("miracastStretch", !stretch))
                }
            for (mode in WirelessPictureMode.entries) {
                val before = store.load()
                assertThrows(IllegalArgumentException::class.java) {
                    store.save(before.copy(wirelessPicture = mode, wirelessPictureSize = WirelessPictureSize(1281, 720)))
                }
                assertEquals(before, store.load())
            }
            context.getSharedPreferences("direlay", 0).edit().putString("wirelessPicture", "future")
                .putInt("wirelessPictureWidth", 0).commit()
            assertEquals(WirelessPictureMode.AUTO, store.load().wirelessPicture)
            assertEquals(WirelessPictureSize(), store.load().wirelessPictureSize)
        }
    }

    @Test fun wirelessDisplaysDefaultOffAndPersistIndependentlyWithParkedReceiverAndPicture() {
        store.use { store ->
            val initial = store.load()
            assertFalse(initial.dlnaOutput); assertFalse(initial.miracastOutput)
            assertNull(initial.dlnaRenderer); assertEquals(DlnaPicture.P720, initial.dlnaPicture)
            for (dlna in listOf(false, true)) for (miracast in listOf(false, true))
                for (picture in DlnaPicture.entries) for (receiver in listOf(null, DlnaRendererChoice("uuid:car", "车机"))) {
                    val value = initial.copy(outputs = OutputSelection.legacy(false, false, dlna, miracast), dlnaPicture = picture, dlnaRenderer = receiver)
                    value.validate(wireless = false)
                    store.save(value)
                    assertEquals(value, store.load())
                    RelaySettingsStore(context).use { assertEquals(value, it.load()) }
                    val prefs = context.getSharedPreferences("direlay", 0)
                    assertEquals(RelayOutput.entries.filter { it in value.outputs }.joinToString(",") { it.wire }, prefs.getString("outputs", null))
                    for (key in listOf("carlinkOutput", "bothOutputs", "dlnaOutput", "miracastOutput")) assertFalse(prefs.contains(key))
                    assertEquals(receiver?.udn, prefs.getString("dlnaRendererUdn", null))
                    assertEquals(receiver?.name, prefs.getString("dlnaRendererName", null))
                    assertEquals(picture.name, prefs.getString("dlnaPicture", null))
                }
            context.getSharedPreferences("direlay", 0).edit().putString("dlnaPicture", "future").commit()
            assertEquals(DlnaPicture.P720, store.load().dlnaPicture)
        }
    }

    @Test fun teslaDefaultsAndEveryModeAndAddressRoundTripInRelayPreferences() {
        RelaySettingsStore(context).use { store ->
            val initial = store.load()
            assertEquals(TeslaBrowserMode.OFF, initial.teslaBrowser)
            assertEquals("169.254.88.1", initial.teslaVirtualAddress)
            assertEquals("169.254.88.1", initial.teslaPublicAddress)
            assertEquals("100.88.0.1", initial.teslaMirrorAddress)
            assertEquals("3.3.3.1", initial.teslaAndroidAddress)
            val addresses = mapOf(TeslaBrowserMode.OFF to null,
                TeslaBrowserMode.VIRTUAL_NAME to "169.254.88.1", TeslaBrowserMode.PUBLIC_ADDRESS to "169.254.88.1",
                TeslaBrowserMode.TESLAMIRROR to "100.88.0.1", TeslaBrowserMode.TESLA_ANDROID to "3.3.3.1")
            for ((mode, address) in addresses) assertEquals(address, initial.copy(teslaBrowser = mode).teslaAddress())
            for (mode in TeslaBrowserMode.entries) {
                val chosen = initial.copy(teslaBrowser = mode, teslaVirtualAddress = "100.88.0.2", teslaPublicAddress = "3.3.3.2",
                    teslaMirrorAddress = "100.88.0.3", teslaAndroidAddress = "3.3.3.3",
                    car = CarSelection("tesla", model = "model-y", year = "model-y-2025"))
                store.save(chosen)
                assertEquals(chosen, store.load())
                val preferences = context.getSharedPreferences("direlay", 0)
                assertEquals(mode.name, preferences.getString("teslaBrowser", null))
                assertEquals("100.88.0.2", preferences.getString("teslaVirtualAddress", null))
                assertEquals("3.3.3.2", preferences.getString("teslaPublicAddress", null))
                assertEquals("100.88.0.3", preferences.getString("teslaMirrorAddress", null))
                assertEquals("3.3.3.3", preferences.getString("teslaAndroidAddress", null))
            }
            assertNotEquals(initial, initial.copy(teslaMirrorAddress = "100.88.0.3"))
            assertNotEquals(initial, initial.copy(teslaAndroidAddress = "3.3.3.3"))
            assertNotEquals(initial, initial.copy(teslaVirtualAddress = "100.88.0.2"))
            assertNotEquals(initial, initial.copy(teslaPublicAddress = "3.3.3.2"))
        }
    }

    @Test fun teslaValidationRejectsASelectedPhoneAddressAndAllowsSwitchingOff() {
        val chosen = store.load().copy(teslaBrowser = TeslaBrowserMode.PUBLIC_ADDRESS)
        assertThrows(IllegalArgumentException::class.java) { chosen.validate(wireless = false, phoneAddresses = setOf("169.254.88.1")) }
        assertThrows(IllegalArgumentException::class.java) { chosen.copy(teslaPublicAddress = "192.168.1.9").validate(wireless = false) }
        chosen.copy(teslaBrowser = TeslaBrowserMode.OFF, teslaPublicAddress = "invalid").validate(wireless = false)
        context.getSharedPreferences("direlay", 0).edit().putString("teslaBrowser", "future-mode").commit()
        assertEquals(TeslaBrowserMode.OFF, store.load().teslaBrowser)
    }

    private fun developerOptions(change: (DeveloperOverrides) -> DeveloperOverrides) {
        RelaySettingsStore(context).use { store ->
            store.saveDeveloperOverrides(change(store.developerOverrides.value))
        }
    }

    @Test fun carlinkPictureSizeDefaultsToWindowAndPersistsEveryOption() {
        developerOptions { it.copy(carlinkPictureSizeEnabled = true) }
        val preferences = context.getSharedPreferences("direlay", 0)
        assertEquals(org.openxplay.stream.clink.ClinkPictureSize.WINDOW, store.load().carlinkPictureSize)
        for (size in org.openxplay.stream.clink.ClinkPictureSize.entries) {
            val settings = store.load().copy(carlinkPictureSize = size)
            store.save(settings)
            assertEquals(size.name, preferences.getString("carlinkPictureSize", null))
            assertEquals(settings, RelaySettingsStore(context).load())
        }
        preferences.edit().putString("carlinkPictureSize", "future").commit()
        assertEquals(org.openxplay.stream.clink.ClinkPictureSize.WINDOW, store.load().carlinkPictureSize)
        preferences.edit().remove("carlinkPictureSize").commit()
        assertEquals(org.openxplay.stream.clink.ClinkPictureSize.WINDOW, store.load().carlinkPictureSize)
    }

    @Test fun viewAreasDefaultOnIgnoreExperimentalKeyAndPersist() {
        val preferences = context.getSharedPreferences("direlay", 0)
        for (old in listOf(false, true)) {
            preferences.edit().putBoolean("viewAreas", old).commit()
            assertTrue(store.load().viewAreas)
        }
        val original = store.load()
        store.save(original.copy(viewAreas = false)); assertFalse(store.load().viewAreas)
        preferences.edit().putBoolean("viewAreas", true).commit()
        assertFalse(store.load().viewAreas)
        store.save(original); assertTrue(store.load().viewAreas)
    }
    @Test fun carSelectionPersistsAndNormalizesOnlyOtherTexts() {
        assertEquals(CarSelection(), store.load().car)
        val original = store.load()
        for (car in listOf(CarSelection("xpeng", model = "g6", year = "g6-2023"), CarSelection("xpeng", model = "other", modelText = " Custom "),
            CarSelection("other", " My brand\n", "other", " My car\t "), CarSelection())) {
            store.save(original.copy(car = car))
            assertEquals(original.copy(car = car.normalized()), store.load())
        }
        store.save(original.copy(car = CarSelection("other", "x".repeat(60), modelText = "y".repeat(60))))
        assertEquals(CarSelection("other", "x".repeat(40), "other", "y".repeat(40), "other"), store.load().car)
        assertEquals(CarSelection(), CarSelection("unknown", "ignored", "other", "ignored").normalized())
    }

    @Test fun legacyCarPreferencesLoadAsModelAndYearAndNextSaveWritesNewKeys() {
        val preferences = context.getSharedPreferences("direlay", 0)
        for ((brand, leaf, model) in listOf(Triple("xpeng", "g6-2023", "g6"), Triple("tesla", "model-3-2017", "model-3"))) {
            preferences.edit().putString("carBrand", brand).putString("carModel", leaf).remove("carYear").remove("carYearText").commit()
            val loaded = store.load()
            assertEquals(CarSelection(brand, model = model, year = leaf), loaded.car)
            assertEquals(leaf, preferences.getString("carModel", null))
            store.save(loaded)
            assertEquals(model, preferences.getString("carModel", null))
            assertEquals(leaf, preferences.getString("carYear", null))
            assertEquals("", preferences.getString("carYearText", null))
        }
        val custom = CarSelection("other", "Brand", "other", "Model", "other", "2026")
        store.save(store.load().copy(car = custom))
        assertEquals(custom, store.load().car)
        assertEquals("2026", preferences.getString("carYearText", null))
    }

    @Before fun clear() {
        for (name in listOf("direlay", "diplay", "xcertplay_airplay")) {
            context.getSharedPreferences(name, 0).edit().clear().commit()
        }
    }

    @Test fun pictureSettingsPersistAndTheWebPortPublishesTheSameValues() {
        RelaySettingsStore(context).use { store ->
            val original = store.awaitSnapshot()
            assertEquals(CarPlayResolution.AUTO, original.carplayResolution)
            assertEquals(CarPlayScreenWidth.MM200, original.carplayScreenWidth)
            val seen = java.util.concurrent.CopyOnWriteArrayList<org.openxplay.stream.web.DisplaySettings>()
            val listener = store.listen { seen.add(it) }
            for (resolution in CarPlayResolution.entries) for (size in CarPlayScreenWidth.entries) {
                val value = original.copy(carplayResolution = resolution, carplayScreenWidth = size)
                store.save(value)
                assertEquals(value, store.load())
                assertEquals(value.displaySettings(), store.current())
                val profile = DisplayProfile(1920, 960, 240)
                store.prepareDisplay(profile, size)
                val expected = com.shilapi.xcertplay.airplay.AirPlayDisplaySettings.sanitizeWidthPhysicalMm(physicalWidth(profile, size))
                assertEquals(expected, AirPlayPersistence.loadWidthPhysicalMm(context))
            }
            store.set(org.openxplay.stream.web.DisplaySettings("720p", 125), org.openxplay.stream.core.Origin("web", "test"))
            assertEquals(CarPlayResolution.P720, store.load().carplayResolution)
            assertEquals(CarPlayScreenWidth.MM125, store.load().carplayScreenWidth)
            assertEquals(store.current(), seen.last())
            listener.close()
            val count = seen.size
            store.save(original)
            assertEquals(count, seen.size)
        }
    }

    @Test fun videoCodecPersistsByNameAndPageChangesPublishToAppListeners() {
        RelaySettingsStore(context).use { store ->
            assertEquals(VideoCodecSetting.AUTO, store.awaitSnapshot().videoCodec)
            val seen = java.util.concurrent.CopyOnWriteArrayList<org.openxplay.stream.web.DisplaySettings>()
            store.listen { seen.add(it) }.use {
                for (codec in VideoCodecSetting.entries) {
                    store.save(store.load().copy(videoCodec = codec))
                    assertEquals(codec.name, context.getSharedPreferences("direlay", 0).getString("videoCodec", null))
                    assertEquals(codec, store.load().videoCodec)
                    assertEquals(codec.wire, store.current().videoCodec)
                }
                store.set(store.current().copy(videoCodec = "h265"), org.openxplay.stream.core.Origin("web", "test"))
                assertEquals(VideoCodecSetting.H265, store.snapshot.value!!.videoCodec)
                assertEquals("h265", seen.last().videoCodec)
            }
        }
        context.getSharedPreferences("direlay", 0).edit().putString("videoCodec", "unknown").commit()
        assertEquals(VideoCodecSetting.AUTO, store.load().videoCodec)
    }

    @Test fun screenWidthUsesNewNumericKeyAndIgnoresOldPreference() {
        val preferences = context.getSharedPreferences("direlay", 0)
        preferences.edit().putString("carplayIconSize", "ignored").commit()
        RelaySettingsStore(context).use { store ->
            assertEquals(CarPlayScreenWidth.MM200, store.load().carplayScreenWidth)
            for (preset in CarPlayScreenWidth.entries) {
                store.save(store.load().copy(carplayScreenWidth = preset))
                assertEquals(preset.mm, preferences.getInt("carplayScreenWidth", 0))
                assertEquals(preset, store.load().carplayScreenWidth)
            }
            preferences.edit().putInt("carplayScreenWidth", 150).commit()
            assertEquals(CarPlayScreenWidth.MM200, store.load().carplayScreenWidth)
        }
    }
    @Test fun developerWidthOverrideWinsForPreparedAndFixedDisplays() {
        RelaySettingsStore(context).use { store ->
            val profile = DisplayProfile(2400, 1200, 300)
            store.physicalWidthOverrideMm = 225
            for (preset in CarPlayScreenWidth.entries) {
                store.prepareDisplay(profile, preset)
                assertEquals(225, AirPlayPersistence.loadWidthPhysicalMm(context))
                store.save(store.load().copy(displayMode = DisplayMode.FIXED, display = profile, carplayScreenWidth = preset))
                assertEquals(225, AirPlayPersistence.loadWidthPhysicalMm(context))
            }
            store.physicalWidthOverrideMm = 0
            store.prepareDisplay(profile, CarPlayScreenWidth.MM300)
            assertEquals(375, AirPlayPersistence.loadWidthPhysicalMm(context))
        }
    }

    @Test fun carlinkHevcDefaultsAutomaticPreservesStoredNamesAndRejectsUnknownNames() {
        developerOptions { it.copy(carlinkHevcEnabled = true) }
        RelaySettingsStore(context).use { store ->
            assertEquals(org.openxplay.stream.clink.ClinkHevcMode.STREAM_ONLY, store.load().carlinkHevc)
            for (mode in org.openxplay.stream.clink.ClinkHevcMode.entries) {
                // Existing installations persisted the enum name, independent of option order.
                context.getSharedPreferences("direlay", 0).edit().putString("carlinkHevc", mode.name).commit()
                assertEquals(mode, store.load().carlinkHevc)
                store.save(store.load().copy(carlinkHevc = mode))
                assertEquals(mode, store.load().carlinkHevc)
            }
            context.getSharedPreferences("direlay", 0).edit().putString("carlinkHevc", "unknown").commit()
            assertEquals(org.openxplay.stream.clink.ClinkHevcMode.STREAM_ONLY, store.load().carlinkHevc)
        }
    }
    @Test fun videoStatsDefaultsOffRoundTripsAndRejectsUnknownValues() {
        RelaySettingsStore(context).use { store ->
            assertEquals(VideoStatsWindow.OFF, store.load().videoStats)
            for (window in VideoStatsWindow.entries) {
                store.save(store.load().copy(videoStats = window))
                assertEquals(window, RelaySettingsStore(context).use { it.load().videoStats })
            }
            context.getSharedPreferences("direlay", 0).edit().putString("videoStats", "unknown").commit()
            assertEquals(VideoStatsWindow.OFF, store.load().videoStats)
            context.getSharedPreferences("direlay", 0).edit().remove("videoStats").commit()
            assertEquals(VideoStatsWindow.OFF, store.load().videoStats)
        }
    }
    @Test fun carlinkTraceDefaultsOffAndPersists() {
        val store = RelaySettingsStore(context)
        val original = store.load()
        assertFalse(original.carlinkTrace)
        store.saveDeveloperOverrides(store.developerOverrides.value.copy(carlinkTrace = true))
        assertTrue(store.load().carlinkTrace)
        store.save(original)
        assertFalse(store.load().carlinkTrace)
    }

    @Test fun carlinkResizeReconnectDefaultsOffAndPersistsIndependentlyOfAutoReconnect() {
        val preferences = context.getSharedPreferences("direlay", 0)
        RelaySettingsStore(context).use { store ->
            val original = store.awaitSnapshot()
            assertFalse(original.carlinkReconnectOnResize)
            for (auto in listOf(false, true)) for (resize in listOf(false, true)) {
                val changed = original.copy(carlinkAutoReconnect = auto, carlinkReconnectOnResize = resize)
                store.save(changed)
                assertEquals(resize, preferences.getBoolean("carlinkReconnectOnResize", !resize))
                assertEquals(changed, store.snapshot.value)
                assertEquals(changed, RelaySettingsStore(context).use { it.load() })
            }
        }
    }

    @Test fun iphoneChoiceDefaultsToAutomaticAndCanBeClearedWithoutChangingOtherSettings() {
        val store = RelaySettingsStore(context)
        val original = store.load()
        assertNull(original.iphoneAddress)
        val chosen = original.copy(iphoneAddress = "00:11:22:33:A9:30")
        store.save(chosen)
        assertEquals(chosen, RelaySettingsStore(context).load())
        store.save(chosen.copy(iphoneAddress = null))
        assertEquals(original, RelaySettingsStore(context).load())
    }

    @Test fun guidedSetupStaysOpenAfterTheGuidesOwnSavesButExistingInstallsCountAsSetUp() {
        val fresh = RelaySettingsStore(context)
        fresh.save(fresh.load().copy(outputs = setOf(RelayOutput.CARLINK)))
        assertFalse(RelaySettingsStore(context).setupCompleted)
        fresh.setupCompleted = true
        assertTrue(RelaySettingsStore(context).setupCompleted)

        clear()
        context.getSharedPreferences("direlay", 0).edit().putBoolean("carlinkOutput", false).commit()
        val existing = RelaySettingsStore(context)
        existing.load()
        assertTrue(existing.setupCompleted)
    }

    @Test fun legacyDisplayPreferencesMigrateOnceAndExplicitRelayScreenSurvives() {
        val preferences = context.getSharedPreferences("direlay", 0)
        assertEquals(DisplayMode.AUTOMATIC, store.load().displayMode)
        preferences.edit().remove("displayMode").putBoolean("customDisplay", false).commit()
        assertEquals(DisplayMode.AUTOMATIC, store.load().displayMode)
        for (profile in DisplayProfile.presets + DisplayProfile(1440, 540, 200)) {
            preferences.edit().remove("displayMode").putBoolean("customDisplay", true)
                .putInt("displayWidth", profile.width).putInt("displayHeight", profile.height)
                .putInt("displayDensity", profile.density).commit()
            val migrated = store.load()
            assertEquals(DisplayMode.FIXED, migrated.displayMode)
            assertEquals(profile, migrated.display)
            assertEquals("FIXED", preferences.getString("displayMode", null))
        }
        store.save(store.load().copy(displayMode = DisplayMode.RELAY_SCREEN, display = null))
        assertEquals(DisplayMode.RELAY_SCREEN, store.load().displayMode)
        store.save(store.load().copy(displayMode = DisplayMode.AUTOMATIC))
        assertEquals(DisplayMode.AUTOMATIC, store.load().displayMode)
        preferences.edit().remove("displayMode").putBoolean("customDisplay", true)
            .putInt("displayWidth", 801).commit()
        assertEquals(DisplayMode.AUTOMATIC, store.load().displayMode)
        assertNull(store.load().display)
    }

    @Test fun vehicleInformationIsOptInAndPersistsIndependentlyOfOutput() {
        val store = RelaySettingsStore(context)
        assertFalse(store.load().vehicleInformation)
        store.save(store.load().copy(outputs = setOf(RelayOutput.CARLINK), vehicleInformation = true))
        assertTrue(RelaySettingsStore(context).load().vehicleInformation)
        store.save(store.load().copy(vehicleInformation = false))
        assertFalse(store.load().vehicleInformation)
        assertTrue(store.load().carlinkOutput)
    }
    @Test fun newInstallAutoConnectDefaultsOnAndExplicitOptOutSurvivesReload() {
        assertTrue(store.load().autoStartWhenReady)
        assertTrue(context.getSharedPreferences("diplay", 0).getBoolean("auto_connect", false))
        store.save(store.load().copy(autoStartWhenReady = false))
        assertFalse(RelaySettingsStore(context).load().autoStartWhenReady)
    }

    @Test fun outputCombinationsIgnoreLegacyDeveloperModeAndLeaveItsStoredKeyUntouched() {
        val preferences = context.getSharedPreferences("direlay", 0)
        for (legacy in listOf(false, true)) {
            preferences.edit().putBoolean("developerMode", legacy).commit()
            RelaySettingsStore(context).use { store ->
                store.save(store.load().copy(outputs = RelayOutput.entries.toSet()))
                assertEquals(RelayOutput.entries.toSet(), store.load().outputs)
                assertEquals(legacy, preferences.getBoolean("developerMode", !legacy))
            }
        }
    }

    @Test fun allLegacyOutputKeysMigrateBeforePersistAndSetupCompletionIsKept() {
        val prefs = context.getSharedPreferences("direlay", 0)
        for (mask in 0..15) {
            clear()
            prefs.edit().putBoolean("carlinkOutput", mask and 1 != 0).putBoolean("bothOutputs", mask and 2 != 0)
                .putBoolean("dlnaOutput", mask and 4 != 0).putBoolean("miracastOutput", mask and 8 != 0).commit()
            RelaySettingsStore(context).use { store ->
                val expected = OutputSelection.legacy(mask and 1 != 0, mask and 2 != 0, mask and 4 != 0, mask and 8 != 0)
                val settings = store.load()
                assertEquals(expected, settings.outputs)
                assertTrue(store.setupCompleted)
                store.save(settings)
                assertEquals(RelayOutput.entries.filter { it in expected }.joinToString(",") { it.wire }, prefs.getString("outputs", null))
                for (key in listOf("carlinkOutput", "bothOutputs", "dlnaOutput", "miracastOutput")) assertFalse(prefs.contains(key))
                assertEquals(expected, store.load().outputs)
                assertTrue(store.setupCompleted)
            }
        }
    }

    @Test fun newStoredOutputsWinDropUnknownValuesAndEmptySetsCannotBeSaved() {
        val prefs = context.getSharedPreferences("direlay", 0)
        for ((wire, expected) in listOf("future,miracast,web" to setOf(RelayOutput.BROWSER, RelayOutput.MIRACAST),
            "dlna,dlna" to setOf(RelayOutput.DLNA), "future" to setOf(RelayOutput.BROWSER), "" to setOf(RelayOutput.BROWSER))) {
            clear()
            prefs.edit().putString("outputs", wire).putBoolean("bothOutputs", true).commit()
            RelaySettingsStore(context).use { store ->
                val settings = store.load()
                assertEquals(expected, settings.outputs)
                assertTrue(store.setupCompleted)
                assertThrows(IllegalArgumentException::class.java) { settings.copy(outputs = emptySet()).validate(false) }
                assertThrows(IllegalArgumentException::class.java) { store.save(settings.copy(outputs = emptySet())) }
                assertEquals(expected, store.load().outputs)
            }
        }
    }

    @Test fun freshIdentityDefaultsAreSavedThroughAirPlayPersistence() {
        store.load()
        assertEquals("OpenXPlay", AirPlayPersistence.loadManufacturer(context))
        assertEquals("OpenXPlay Relay", AirPlayPersistence.loadModel(context))
        assertEquals("OpenXPlay", AirPlayPersistence.loadOemLabel(context))
        store.load()
        assertEquals("OpenXPlay", AirPlayPersistence.loadManufacturer(context))
    }

    @Test fun storedIdentityValuesArePreservedIndependently() {
        AirPlayPersistence.saveManufacturer(context, "My manufacturer")
        store.load()
        assertEquals("My manufacturer", AirPlayPersistence.loadManufacturer(context))
        assertEquals("OpenXPlay Relay", AirPlayPersistence.loadModel(context))
        context.getSharedPreferences("xcertplay_airplay", 0).edit().remove("manufacturer").commit()
        AirPlayPersistence.saveModel(context, "My model")
        store.load()
        assertEquals("OpenXPlay", AirPlayPersistence.loadManufacturer(context))
        assertEquals("My model", AirPlayPersistence.loadModel(context))
        AirPlayPersistence.saveManufacturer(context, "DiPlay")
        store.load()
        assertEquals("DiPlay", AirPlayPersistence.loadManufacturer(context))
        assertEquals("My model", AirPlayPersistence.loadModel(context))
    }

    @Test fun audioDefaultsAndBothMicrophoneSourcesPersist() {
        assertFalse(store.load().playCarPlayAudio)
        assertEquals(MicrophoneSource.HEAD_UNIT, store.load().microphoneSource)
        developerOptions { it.copy(microphoneSourceEnabled = true) }
        for (source in MicrophoneSource.entries) for (play in listOf(false, true)) {
            val changed = store.load().copy(playCarPlayAudio = play, microphoneSource = source)
            developerOptions { it.copy(playCarPlayAudio = play) }
            store.save(changed)
            assertEquals(changed, RelaySettingsStore(context).load())
        }
        context.getSharedPreferences("direlay", 0).edit().putString("microphoneSource", "future").commit()
        assertEquals(MicrophoneSource.HEAD_UNIT, store.load().microphoneSource)
    }

    @Test fun carBluetoothAudioDefaultsOnAndLivesInDiPlayPersistence() {
        assertTrue(store.load().carBluetoothAudio)
        for (enabled in listOf(true, false)) {
            store.save(store.load().copy(carBluetoothAudio = enabled))
            assertEquals(enabled, AirPlayPersistence.loadCarBluetoothAudio(context))
            assertEquals(enabled, RelaySettingsStore(context).load().carBluetoothAudio)
        }
        AirPlayPersistence.saveCarBluetoothAudio(context, true)
        assertTrue(store.load().carBluetoothAudio)
    }

    @Test fun carBluetoothAudioIsDefaultedSynchronouslyBeforeTheFirstSettingsLoad() {
        val preferences = context.getSharedPreferences("direlay", 0)
        assertFalse(preferences.contains("carBluetoothAudioDefaulted"))
        RelaySettingsStore(context).use { store ->
            assertNull(store.snapshot.value)
            assertTrue(AirPlayPersistence.loadCarBluetoothAudio(context))
            assertTrue(preferences.getBoolean("carBluetoothAudioDefaulted", false))
        }
    }

    @Test fun carBluetoothAudioUpgradeEnablesStoredValuesOnceWhenTheMarkerIsMissing() {
        for (stored in listOf(false, true)) {
            clear()
            AirPlayPersistence.saveCarBluetoothAudio(context, stored)
            RelaySettingsStore(context).use { store ->
                assertTrue(AirPlayPersistence.loadCarBluetoothAudio(context))
                assertTrue(context.getSharedPreferences("direlay", 0).getBoolean("carBluetoothAudioDefaulted", false))
                store.save(store.load().copy(carBluetoothAudio = false))
            }
            RelaySettingsStore(context).use { store ->
                assertFalse(AirPlayPersistence.loadCarBluetoothAudio(context))
                assertFalse(store.load().carBluetoothAudio)
            }
        }
    }

    @Test fun carBluetoothAudioMarkerPreservesEitherUserChoiceRegardlessOfMarkerValue() {
        val preferences = context.getSharedPreferences("direlay", 0)
        for (marker in listOf(false, true)) for (stored in listOf(false, true)) {
            preferences.edit().putBoolean("carBluetoothAudioDefaulted", marker).commit()
            AirPlayPersistence.saveCarBluetoothAudio(context, stored)
            RelaySettingsStore(context).use { store ->
                assertEquals(stored, AirPlayPersistence.loadCarBluetoothAudio(context))
                assertEquals(stored, store.load().carBluetoothAudio)
                assertEquals(marker, preferences.getBoolean("carBluetoothAudioDefaulted", !marker))
            }
        }
    }

    @Test fun sharedValuesAreWrittenThroughAirPlayPersistenceAndReadBack() {
        for (target in MfiTarget.entries) {
            val settings = store.load().copy(
                mfiTarget = target, i2cPath = " /dev/i2c-3 ",
                remoteServer = " https://mfi.example.test ", remoteToken = "private-token",
                frameRate = FrameRatePreference.FPS_55, autoStartOnBoot = true,
            )
            store.save(settings)
            assertEquals(target, AirPlayPersistence.loadMfiTarget(context))
            assertEquals("/dev/i2c-3", AirPlayPersistence.loadMfiI2cPath(context))
            assertEquals("https://mfi.example.test", AirPlayPersistence.loadRemoteMfiServer(context))
            assertEquals("private-token", AirPlayPersistence.loadRemoteMfiToken(context))
            assertEquals(30, AirPlayPersistence.loadFps(context))
            assertTrue(AirPlayPersistence.loadAutoStartOnBoot(context))
            assertEquals(settings.copy(i2cPath = "/dev/i2c-3", remoteServer = "https://mfi.example.test"), store.load())
        }
    }

    @Test fun valuesChangedByDiPlayRemainVisibleInRelay() {
        AirPlayPersistence.saveMfiTarget(context, MfiTarget.I2C)
        AirPlayPersistence.saveMfiI2cPath(context, "/dev/i2c-7")
        AirPlayPersistence.saveRemoteMfiServer(context, "http://mfi.example.test")
        AirPlayPersistence.saveRemoteMfiToken(context, "changed")
        AirPlayPersistence.saveFps(context, 42)
        AirPlayPersistence.saveAutoStartOnBoot(context, true)
        val loaded = store.load()
        assertEquals(MfiTarget.I2C, loaded.mfiTarget)
        assertEquals("/dev/i2c-7", loaded.i2cPath)
        assertEquals("http://mfi.example.test", loaded.remoteServer)
        assertEquals("changed", loaded.remoteToken)
        assertEquals(FrameRatePreference.AUTO, loaded.frameRate)
        assertTrue(loaded.autoStartOnBoot)
    }

    @Test fun defaultModesUseDiPlayKeysAndLastUsedRetainsBothTransports() {
        for (mode in ConnectionMode.entries) {
            for (lastUsed in listOf(false, true)) {
                AirPlayPersistence.saveWirelessEnabled(context, lastUsed)
                store.save(store.load().copy(connectionMode = mode,
                    wirelessNetwork = WirelessNetworkSettings(manualSsid = "Car hotspot")))
                assertEquals(mode.key, context.getSharedPreferences("diplay", 0).getString("default_connection_mode", null))
                assertEquals(mode, store.load().connectionMode)
                val expected = mode.wireless(lastUsed)
                assertEquals(expected, store.wireless())
                assertEquals(expected, AirPlayPersistence.loadWirelessEnabled(context))
                // A fixed default must still win if DiPlay subsequently records another transport.
                AirPlayPersistence.saveWirelessEnabled(context, !lastUsed)
                store.prepareStart()
                assertEquals(mode.wireless(!lastUsed), AirPlayPersistence.loadWirelessEnabled(context))
            }
        }
        context.getSharedPreferences("diplay", 0).edit().putString("default_connection_mode", "future").commit()
        assertEquals(ConnectionMode.LAST_USED, store.load().connectionMode)
    }

    @Test fun headUnitProfilesPersistExactPixelsAndDpiAndMapSupportedPhysicalSize() {
        for (profile in DisplayProfile.presets + DisplayProfile(1440, 540, 200)) {
            AirPlayPersistence.saveMaximumDetectedDisplay(context, 3000, 2000)
            store.save(store.load().copy(display = profile, displayMode = DisplayMode.FIXED))
            assertEquals(profile, store.load().display)
            assertEquals(100, AirPlayPersistence.loadDisplayScalePercent(context))
            assertEquals(AirPlayPhysicalSizeBasis.WIDTH, AirPlayPersistence.loadPhysicalSizeBasis(context))
            assertEquals(0 to 0, AirPlayPersistence.loadMaximumDetectedDisplay(context))
            // The default 200 mm screen width, widened only for canvases wider than 1920 px.
            val expected = (kotlin.math.ceil(200 * maxOf(1.0, profile.width / 1920.0) / 25).toInt() * 25).coerceIn(100, 400)
            assertEquals(expected, AirPlayPersistence.loadWidthPhysicalMm(context))
        }
        store.save(store.load().copy(display = null, displayMode = DisplayMode.AUTOMATIC))
        assertNull(store.load().display)
    }

    @Test fun displayModeAndEffectiveCanvasResetDetectionWithoutAffectingUnrelatedSaves() {
        val fixed = store.load().copy(display = CarDisplayFit.default, displayMode = DisplayMode.FIXED)
        store.save(fixed)
        AirPlayPersistence.saveMaximumDetectedDisplay(context, 1920, 720)
        store.save(fixed.copy(autoStartWhenReady = !fixed.autoStartWhenReady))
        assertEquals(1920 to 720, AirPlayPersistence.loadMaximumDetectedDisplay(context))
        store.save(store.load().copy(displayMode = DisplayMode.AUTOMATIC, display = null))
        assertEquals(0 to 0, AirPlayPersistence.loadMaximumDetectedDisplay(context))
        AirPlayPersistence.saveMaximumDetectedDisplay(context, 3000, 2000)
        store.prepareDisplay(DisplayProfile(1920, 720, 240))
        assertEquals(0 to 0, AirPlayPersistence.loadMaximumDetectedDisplay(context))
        assertEquals(100, AirPlayPersistence.loadDisplayScalePercent(context))
        assertEquals(DisplayMode.AUTOMATIC, store.load().displayMode)
        assertNull(store.load().display)
    }

    @Test fun relayOptionsAndAutoConnectReuseTheirExistingStores() {
        developerOptions { it.copy(localPreview = true, webPortEnabled = true) }
        store.save(store.load().copy(localPreview = true, webPort = 9443, autoStartWhenReady = true))
        val relay = context.getSharedPreferences("direlay", 0)
        assertTrue(relay.getBoolean("localPreview", false))
        assertEquals(9443, relay.getInt("webPort", 0))
        assertTrue(context.getSharedPreferences("diplay", 0).getBoolean("auto_connect", false))
        assertTrue(store.load().autoStartWhenReady)
        assertTrue(store.load().localPreview)
        assertEquals(9443, store.load().webPort)
        store.save(store.load().copy(webPort = null, autoStartWhenReady = false, localPreview = false, autoStartOnBoot = false))
        assertNull(store.load().webPort)
        assertFalse(store.load().autoStartWhenReady)
        assertFalse(store.load().localPreview)
        assertFalse(AirPlayPersistence.loadAutoStartOnBoot(context))
    }

    @Test fun invalidInputsCannotPartiallyOverwriteSettings() {
        developerOptions { it.copy(webPortEnabled = true) }
        val original = store.load()
        val invalid = listOf(
            original.copy(displayMode = DisplayMode.FIXED, display = null),
            original.copy(webPort = 80), original.copy(webPort = 65536),
            original.copy(display = DisplayProfile(801, 480, 160)),
            original.copy(display = DisplayProfile(1280, 720, 0)),
            original.copy(mfiTarget = MfiTarget.I2C, i2cPath = ""),
            original.copy(mfiTarget = MfiTarget.REMOTE, remoteServer = "file:///tmp/key"),
            original.copy(remoteToken = "bad\u0000token"),
        )
        for (settings in invalid) {
            assertTrue(runCatching { store.save(settings.copy(autoStartOnBoot = true)) }.isFailure)
            assertEquals(original, store.load())
        }
    }

    @Test fun startLeavesVideoCodecChoiceToRuntime() {
        AirPlayPersistence.saveHevcEnabled(context, true)
        AirPlayPersistence.saveFps(context, 50)
        store.prepareStart()
        assertTrue(AirPlayPersistence.loadHevcEnabled(context))
        assertEquals(50, AirPlayPersistence.loadFps(context))
    }

    @Test fun cachedSnapshotIsStableOnReadsAndPublishesSavesAndExternalChanges() {
        developerOptions { it.copy(webPortEnabled = true) }
        val observed = RelaySettingsStore(context)
        try {
            // Production initializes on the settings worker; reads below only inspect memory.
            val first = observed.awaitSnapshot()
            repeat(100) { assertSame(first, observed.snapshot.value) }
            val saved = first.copy(carlinkDeviceName = "My car", webPort = 9443, displayMode = DisplayMode.RELAY_SCREEN)
            observed.save(saved)
            assertEquals(saved, observed.snapshot.value)
            assertEquals(saved, observed.load())
            context.getSharedPreferences("direlay", 0).edit().putBoolean("vehicleInformation", true).commit()
            context.getSharedPreferences("diplay", 0).edit().putBoolean("auto_connect", false).commit()
            context.getSharedPreferences("direlay", 0).edit().putString("frameRate", "FPS_55").commit()
            shadowOf(Looper.getMainLooper()).idle()
            val external = runBlocking {
                withTimeout(5_000) { observed.snapshot.filterNotNull().first {
                    it.vehicleInformation && !it.autoStartWhenReady && it.frameRate == FrameRatePreference.FPS_55
                } }
            }
            assertEquals(saved.copy(vehicleInformation = true, autoStartWhenReady = false, frameRate = FrameRatePreference.FPS_55), external)
            repeat(100) { assertSame(external, observed.snapshot.value) }
        } finally { observed.close() }
    }

    @Test fun storedBrowserPairingPreferenceIsIgnored() {
        val before = store.load()
        context.getSharedPreferences("direlay", android.content.Context.MODE_PRIVATE).edit().putBoolean("requirePairingCode", true).commit()
        assertEquals(before, RelaySettingsStore(context).load())
    }
    @Test fun experimentalDefaultsDoNotWritePreferencesAndParkedChoicesPersistUnderTheirOwnKeys() {
        store.use { store ->
            val initial = store.load()
            val prefs = context.getSharedPreferences("direlay", 0)
            assertEquals(ExperimentalOptions(), initial.experimental)
            assertTrue(prefs.all.keys.none { it.startsWith("experimental.") })
            store.save(initial.copy(autoStartOnBoot = !initial.autoStartOnBoot))
            assertTrue(prefs.all.keys.none { it.startsWith("experimental.") })
            val choices = ExperimentalOptions(active = false, rememberSubscription = true, keepScanning = true,
                recordAdvertisements = true, sendBluetoothAddress = true,
                isolateCarlinkNetwork = true, waitForCarlink = true, recoverWirelessNetwork = true, bluetoothAddress = "A0:B1:C2:D3:E4:F5")
            store.save(store.load().copy(experimental = choices))
            RelaySettingsStore(context).use { assertEquals(choices, it.load().experimental) }
            assertEquals(false, prefs.getBoolean("experimental.active", true))
            assertFalse(prefs.contains("experimental.rememberSubscription"))
            assertFalse(prefs.contains("experimental.keepScanning"))
            assertFalse(prefs.contains("experimental.recordAdvertisements"))
            assertEquals(true, prefs.getBoolean("experimental.sendBluetoothAddress", false))
            assertFalse(prefs.contains("experimental.isolateCarlinkNetwork"))
            assertFalse(prefs.contains("experimental.waitForCarlink"))
            assertFalse(prefs.contains("experimental.recoverWirelessNetwork"))
            assertEquals(choices.bluetoothAddress, prefs.getString("experimental.bluetoothAddress", null))
            val writes = mutableListOf<String>()
            val listener = android.content.SharedPreferences.OnSharedPreferenceChangeListener { _, key -> key?.let(writes::add) }
            prefs.registerOnSharedPreferenceChangeListener(listener)
            try {
                store.save(store.load().copy(autoStartOnBoot = initial.autoStartOnBoot))
                assertTrue(writes.none { it.startsWith("experimental.") })
            } finally { prefs.unregisterOnSharedPreferenceChangeListener(listener) }
        }
    }

    @Test fun upgradingDisabledLegacyLabsEnablesNewDefaultsOnceWithoutActivatingBluetoothOverride() {
        val prefs = context.getSharedPreferences("direlay", 0)
        prefs.edit().putBoolean("experimental.active", false).putBoolean("experimental.keepScanning", false)
            .putBoolean("experimental.sendBluetoothAddress", true).putString("experimental.bluetoothAddress", "a0:b1:c2:d3:e4:f5").commit()
        RelaySettingsStore(context).use { migrated ->
            val value = migrated.load()
            assertTrue(value.experimental.active)
            assertEquals(13, value.experimental.optionCount)
            assertTrue(value.experimental.effective().keepScanning)
            assertFalse(value.experimental.sendBluetoothAddress)
            migrated.save(value.copy(experimental = value.experimental.copy(active = false, startupDiagnostics = false)))
        }
        RelaySettingsStore(context).use {
            assertFalse(it.load().experimental.active)
            assertFalse(it.load().experimental.startupDiagnostics)
        }
    }


    @Test fun screenExperimentsDefaultOnForExistingVersionTwoAndPersistEachChoice() {
        val prefs = context.getSharedPreferences("direlay", 0)
        prefs.edit().putInt("experimental.version", 2).putBoolean("experimental.active", true).commit()
        RelaySettingsStore(context).use { upgraded ->
            val defaults = upgraded.load().experimental
            assertTrue(defaults.screenSessionIsolation && defaults.screenReconnectReset && defaults.screenDeliverySerialization)
            for (bits in 0..7) {
                val choice = defaults.copy(screenSessionIsolation = bits and 1 != 0,
                    screenReconnectReset = bits and 2 != 0, screenDeliverySerialization = bits and 4 != 0)
                upgraded.save(upgraded.load().copy(experimental = choice))
                RelaySettingsStore(context).use { restarted -> assertEquals(choice, restarted.load().experimental) }
            }
        }
        prefs.edit().putBoolean("experimental.active", false)
            .remove("experimental.screenSessionIsolation").remove("experimental.screenReconnectReset")
            .remove("experimental.screenDeliverySerialization").commit()
        RelaySettingsStore(context).use { restarted ->
            val options = restarted.load().experimental
            assertFalse(options.active)
            assertTrue(options.screenSessionIsolation && options.screenReconnectReset && options.screenDeliverySerialization)
            assertFalse(options.effective().screenSessionIsolation)
        }
    }

    @Test fun handoffExperimentsUseTheirDefaultsForExistingVersionTwoAndPersistEachParkedChoice() {
        val prefs = context.getSharedPreferences("direlay", 0)
        for (active in listOf(false, true)) {
            prefs.edit().putInt("experimental.version", 2).putBoolean("experimental.active", active)
                .remove("experimental.iapChannelWhenOffered").remove("experimental.releaseBluetoothOnRequest")
                .remove("experimental.awaitIphoneReconnect").commit()
            RelaySettingsStore(context).use { upgraded ->
                val defaults = upgraded.load().experimental
                assertEquals(active, defaults.active)
                assertTrue(defaults.iapChannelWhenOffered && defaults.releaseBluetoothOnRequest && defaults.awaitIphoneReconnect)
                assertEquals(active, defaults.effective().iapChannelWhenOffered)
                assertEquals(active, defaults.effective().releaseBluetoothOnRequest)
                assertEquals(active, defaults.effective().awaitIphoneReconnect)
                for (bits in 0..7) {
                    val choice = defaults.copy(iapChannelWhenOffered = bits and 1 != 0,
                        releaseBluetoothOnRequest = bits and 2 != 0, awaitIphoneReconnect = bits and 4 != 0)
                    upgraded.save(upgraded.load().copy(experimental = choice))
                    assertEquals(choice.iapChannelWhenOffered, prefs.getBoolean("experimental.iapChannelWhenOffered", !choice.iapChannelWhenOffered))
                    assertEquals(choice.releaseBluetoothOnRequest, prefs.getBoolean("experimental.releaseBluetoothOnRequest", !choice.releaseBluetoothOnRequest))
                    assertEquals(choice.awaitIphoneReconnect, prefs.getBoolean("experimental.awaitIphoneReconnect", !choice.awaitIphoneReconnect))
                    assertEquals(2, prefs.getInt("experimental.version", 0))
                    RelaySettingsStore(context).use { restarted -> assertEquals(choice, restarted.load().experimental) }
                }
            }
        }
    }

    @Test fun diagnosticsExperimentsStartOnForExistingSettingsAndPersistEachChoice() {
        val prefs = context.getSharedPreferences("direlay", 0)
        prefs.edit().putInt("experimental.version", 2).putBoolean("experimental.active", true)
            .remove("experimental.radioConflictPrompt").remove("experimental.stallWatchdog").commit()
        RelaySettingsStore(context).use { upgraded ->
            val defaults = upgraded.load().experimental
            assertTrue(defaults.radioConflictPrompt && defaults.stallWatchdog)
            for (bits in 0..3) {
                val choice = defaults.copy(radioConflictPrompt = bits and 1 != 0, stallWatchdog = bits and 2 != 0)
                upgraded.save(upgraded.load().copy(experimental = choice))
                assertEquals(choice.radioConflictPrompt, prefs.getBoolean("experimental.radioConflictPrompt", !choice.radioConflictPrompt))
                assertEquals(choice.stallWatchdog, prefs.getBoolean("experimental.stallWatchdog", !choice.stallWatchdog))
                RelaySettingsStore(context).use { restarted -> assertEquals(choice, restarted.load().experimental) }
            }
        }
    }
}
