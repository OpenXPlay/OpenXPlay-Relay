// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-15
package org.openxplay.direlay.ui

import android.app.Application
import android.os.Bundle
import android.os.Parcel
import androidx.compose.runtime.saveable.SaverScope
import com.shilapi.xcertplay.orchestration.MfiTarget
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.openxplay.direlay.settings.*

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [33], manifest = Config.NONE, application = Application::class)
class SettingsDraftSaverTest {
    private val base = RelaySettings(ConnectionMode.WIRELESS, MfiTarget.LOCAL, "", "", "", null,
        30, false, true, false, null)
    private val scope = object : SaverScope { override fun canBeSaved(value: Any) = true }
    private fun bundle(value: RelaySettings): Bundle = with(relaySettingsSaver(base)) { requireNotNull(scope.save(value)) }

    @Test fun wirelessPictureChoicesUseOwnKeysAndSurviveParcelRoundTrip() {
        for (mode in WirelessPictureMode.entries) for (stretch in listOf(false, true)) {
            val chosen = base.copy(wirelessPicture = mode, wirelessPictureSize = WirelessPictureSize(1920, 720), miracastStretch = stretch)
            val saved = bundle(chosen)
            assertEquals(7, requireNotNull(saved.getBooleanArray("flags")).size)
            val parcel = Parcel.obtain()
            try {
                parcel.writeBundle(saved); parcel.setDataPosition(0)
                assertEquals(chosen, relaySettingsSaver(base).restore(requireNotNull(parcel.readBundle(javaClass.classLoader))))
            } finally { parcel.recycle() }
        }
        val old = bundle(base).apply {
            listOf("wirelessPicture", "wirelessPictureWidth", "wirelessPictureHeight", "miracastStretch").forEach { remove(it) }
        }
        val remembered = base.copy(wirelessPicture = WirelessPictureMode.CUSTOM, wirelessPictureSize = WirelessPictureSize(720, 1280), miracastStretch = true)
        assertEquals(remembered, relaySettingsSaver(remembered).restore(old))
        old.putString("wirelessPicture", "future")
        assertEquals(base, relaySettingsSaver(base).restore(old))
    }

    @Test fun wirelessDisplaysSurviveParcelRoundTripWithoutExtendingLegacyFlags() {
        for (dlna in listOf(false, true)) for (miracast in listOf(false, true))
            for (picture in DlnaPicture.entries) for (receiver in listOf(null, DlnaRendererChoice("uuid:car", "车机"))) {
                val value = base.copy(dlnaOutput = dlna, miracastOutput = miracast, dlnaPicture = picture, dlnaRenderer = receiver)
                val saved = bundle(value)
                assertEquals(7, requireNotNull(saved.getBooleanArray("flags")).size)
                val parcel = Parcel.obtain()
                try {
                    parcel.writeBundle(saved); parcel.setDataPosition(0)
                    assertEquals(value, relaySettingsSaver(base).restore(requireNotNull(parcel.readBundle(javaClass.classLoader))))
                } finally { parcel.recycle() }
            }
        val old = bundle(base).apply {
            listOf("dlnaOutput", "dlnaRendererUdn", "dlnaRendererName", "dlnaPicture", "miracastOutput").forEach { remove(it) }
        }
        val remembered = base.copy(dlnaOutput = true, miracastOutput = true,
            dlnaPicture = DlnaPicture.P1080, dlnaRenderer = DlnaRendererChoice("uuid:remembered", "Remembered"))
        assertEquals(remembered, relaySettingsSaver(remembered).restore(old))
        old.putString("dlnaPicture", "future")
        assertEquals(base, relaySettingsSaver(base).restore(old))
        assertEquals(base, relaySettingsSaver(remembered).restore(bundle(base)))
    }

    @Test fun teslaModeAndAllAddressesSurviveRotationAndOlderDraftsKeepDefaults() {
        for (mode in TeslaBrowserMode.entries) {
            val chosen = base.copy(teslaBrowser = mode, teslaVirtualAddress = "100.88.0.2", teslaPublicAddress = "3.3.3.2",
                teslaMirrorAddress = "100.88.0.3", teslaAndroidAddress = "3.3.3.3")
            val parcel = Parcel.obtain()
            try {
                parcel.writeBundle(bundle(chosen)); parcel.setDataPosition(0)
                assertEquals(chosen, relaySettingsSaver(base).restore(requireNotNull(parcel.readBundle(javaClass.classLoader))))
            } finally { parcel.recycle() }
        }
        val old = bundle(base).apply {
            remove("teslaBrowser"); remove("teslaVirtualAddress"); remove("teslaPublicAddress")
            remove("teslaMirrorAddress"); remove("teslaAndroidAddress")
        }
        assertEquals(base, relaySettingsSaver(base).restore(old))
    }

    @Test fun rememberedDeveloperValuesAndSwitchesSurviveARealBundleRoundTrip() {
        val chosen = DeveloperOverrides(carlinkTrace = true,
            carlinkHevcEnabled = true, carlinkHevc = org.openxplay.stream.clink.ClinkHevcMode.OFF,
            carlinkPictureSizeEnabled = true, carlinkPictureSize = org.openxplay.stream.clink.ClinkPictureSize.MODE,
            physicalWidthEnabled = true, physicalWidthOverrideMm = 225, fpsEnabled = true, fps = 55,
            localPreview = true, playCarPlayAudio = true, microphoneSourceEnabled = true,
            microphoneSource = MicrophoneSource.THIS_DEVICE, webPortEnabled = true, webPort = 9443,
            carlinkDeviceModelEnabled = true, carlinkDeviceModel = "Test relay")
        val saver = developerOverridesSaver(DeveloperOverrides())
        for (value in listOf(chosen, chosen.copy(active = false), chosen.copy(carlinkPictureSizeEnabled = false, webPortEnabled = false))) {
            val encoded = with(saver) { requireNotNull(scope.save(value)) }
            val parcel = Parcel.obtain()
            try {
                encoded.writeToParcel(parcel, 0)
                parcel.setDataPosition(0)
                assertEquals(value, saver.restore(Bundle.CREATOR.createFromParcel(parcel)))
            } finally { parcel.recycle() }
        }
        assertEquals(chosen, developerOverridesSaver(chosen).restore(Bundle()))
    }

    @Test fun carlinkPictureSizeUsesWindowModeOrderAndEveryDraftSurvivesRotation() {
        assertEquals(listOf("WINDOW", "MODE"), org.openxplay.stream.clink.ClinkPictureSize.entries.map { it.name })
        assertEquals(org.openxplay.stream.clink.ClinkPictureSize.WINDOW, base.carlinkPictureSize)
        for (size in org.openxplay.stream.clink.ClinkPictureSize.entries) {
            val value = base.copy(carlinkPictureSize = size)
            val saved = bundle(value)
            assertEquals(size.name, saved.getString("carlinkPictureSize"))
            assertEquals(value, relaySettingsSaver(base).restore(saved))
        }
        val old = bundle(base); old.remove("carlinkPictureSize")
        val remembered = base.copy(carlinkPictureSize = org.openxplay.stream.clink.ClinkPictureSize.MODE)
        assertEquals(remembered, relaySettingsSaver(remembered).restore(old))
        old.putString("carlinkPictureSize", "future")
        assertEquals(base, relaySettingsSaver(base).restore(old))
    }
    @Test fun carlinkHevcOptionsUseAutomaticOffSignalOrder() {
        assertEquals(listOf("STREAM_ONLY", "OFF", "SIGNAL"), org.openxplay.stream.clink.ClinkHevcMode.entries.map { it.name })
        assertEquals(org.openxplay.stream.clink.ClinkHevcMode.STREAM_ONLY, base.carlinkHevc)
    }
    @Test fun everyCarlinkHevcDraftSurvivesRotation() {
        for (mode in org.openxplay.stream.clink.ClinkHevcMode.entries) {
            val value = base.copy(carlinkHevc = mode)
            assertEquals(value, relaySettingsSaver(base).restore(bundle(value)))
        }
        val old = bundle(base); old.remove("carlinkHevc")
        assertEquals(base, relaySettingsSaver(base).restore(old))
    }
    @Test fun carBluetoothAudioDefaultsOnAndDraftSurvivesRotationAndOldDraftsKeepTheBase() {
        assertTrue(base.carBluetoothAudio)
        for (enabled in listOf(true, false)) {
            val value = base.copy(carBluetoothAudio = enabled)
            assertEquals(value, relaySettingsSaver(base).restore(bundle(value)))
        }
        val old = bundle(base); old.remove("carBluetoothAudio")
        val remembered = base.copy(carBluetoothAudio = false)
        assertEquals(remembered, relaySettingsSaver(remembered).restore(old))
    }
    @Test fun resizeReconnectDraftSurvivesRotationAndOldDraftsKeepTheBase() {
        assertFalse(base.carlinkReconnectOnResize)
        for (enabled in listOf(true, false)) {
            val value = base.copy(carlinkReconnectOnResize = enabled)
            assertEquals(value, relaySettingsSaver(base).restore(bundle(value)))
        }
        val old = bundle(base); old.remove("carlinkReconnectOnResize")
        val remembered = base.copy(carlinkReconnectOnResize = true)
        assertEquals(remembered, relaySettingsSaver(remembered).restore(old))
    }
    @Test fun everyVideoStatsWindowSurvivesRotation() {
        for (window in VideoStatsWindow.entries) {
            val value = base.copy(videoStats = window)
            assertEquals(value, relaySettingsSaver(base).restore(bundle(value)))
        }
        val old = bundle(base); old.remove("videoStats")
        val remembered = base.copy(videoStats = VideoStatsWindow.SECONDS_15)
        assertEquals(remembered, relaySettingsSaver(remembered).restore(old))
        old.putString("videoStats", "unknown")
        assertEquals(base, relaySettingsSaver(base).restore(old))
    }
    @Test fun everyVideoCodecDraftSurvivesRotationAndOldDraftsKeepTheBase() {
        for (codec in VideoCodecSetting.entries) {
            val value = base.copy(videoCodec = codec)
            val saved = bundle(value)
            assertEquals(codec.name, saved.getString("videoCodec"))
            assertEquals(value, relaySettingsSaver(base).restore(saved))
        }
        val old = bundle(base); old.remove("videoCodec")
        val remembered = base.copy(videoCodec = VideoCodecSetting.H265)
        assertEquals(remembered, relaySettingsSaver(remembered).restore(old))
    }
    @Test fun everyScreenWidthDraftUsesTheNewNumericKey() {
        for (width in CarPlayScreenWidth.entries) {
            val value = base.copy(carplayScreenWidth = width)
            val saved = bundle(value)
            assertEquals(width.mm, saved.getInt("carplayScreenWidth"))
            assertEquals(value, relaySettingsSaver(base).restore(saved))
        }
        val old = bundle(base)
        old.remove("carplayScreenWidth"); old.putString("carplayIconSize", "ignored")
        assertEquals(base, relaySettingsSaver(base).restore(old))
    }
    @Test fun carAndViewAreasDraftRoundTripAndOldExperimentalKeyIsIgnored() {
        for (areas in listOf(false, true)) for (car in listOf(CarSelection(), CarSelection("xpeng", model = "g6-2023"),
            CarSelection("other", "My brand ", "other", "My model "))) {
            val value = base.copy(viewAreas = areas, car = car)
            assertEquals(value, relaySettingsSaver(base).restore(bundle(value)))
        }
        val old = bundle(base)
        old.remove("carPlayViewAreas"); old.remove("car"); old.putBoolean("viewAreas", false)
        assertEquals(base, relaySettingsSaver(base).restore(old))
    }
    @Test fun everyNetworkChoiceAndPhoneSelectionRoundTripWithCredentials() {
        for (choice in WirelessNetworkChoice.entries) for (selected in listOf(false, true)) {
            val value = base.copy(carlinkAutoReconnect = selected, iphoneAddress = if (selected) "00:11:22:33:A9:30" else null, wirelessNetwork = WirelessNetworkSettings(
                choice = choice, manualSsid = "Phone", manualPassphrase = "12345678",
                sharedWifiNetworks = listOf(SharedWifiNetwork("Router", "abcdefgh"), SharedWifiNetwork("Car", ""))))
            assertEquals(value, relaySettingsSaver(base).restore(bundle(value)))
        }
    }
    @Test fun oldCarHotspotNameAndShortBundlesMigrate() {
        val bundle = bundle(base)
        val text = requireNotNull(bundle.getStringArrayList("text"))
        text[8] = "CAR_HOTSPOT"
        bundle.putStringArrayList("text", text)
        bundle.putBooleanArray("flags", requireNotNull(bundle.getBooleanArray("flags")).copyOf(7))
        val restored = requireNotNull(relaySettingsSaver(base).restore(bundle))
        assertEquals(WirelessNetworkChoice.PHONE_HOTSPOT, restored.wirelessNetwork.choice)
        assertTrue(restored.carlinkAutoReconnect)
    }
    @Test fun oldDraftFlagsKeepCarlinkReconnectAndIgnoreRemovedSlot() {
        for (removed in listOf(false, true)) for (reconnect in listOf(false, true)) {
            val bundle = bundle(base)
            bundle.remove("carlinkAutoReconnect")
            bundle.putBooleanArray("flags", requireNotNull(bundle.getBooleanArray("flags")) + booleanArrayOf(removed, reconnect))
            assertEquals(base.copy(carlinkAutoReconnect = reconnect), relaySettingsSaver(base).restore(bundle))
        }
    }
    @Test fun oldSingleNetworkDraftMigrates() {
        val bundle = bundle(base)
        val text = requireNotNull(bundle.getStringArrayList("text"))
        text[13] = "Legacy"; text.add("legacy-password")
        bundle.putStringArrayList("text", text)
        assertEquals(listOf(SharedWifiNetwork("Legacy", "legacy-password")),
            requireNotNull(relaySettingsSaver(base).restore(bundle)).wirelessNetwork.sharedWifiNetworks)
    }
    @Test fun experimentalChoicesAndIncompleteAddressSurviveParcelRoundTrip() {
        val options = ExperimentalOptions(active = false, rememberSubscription = true, keepScanning = true,
            recordAdvertisements = true, sendBluetoothAddress = true, bluetoothAddress = "A0:B1:")
        val chosen = base.copy(experimental = options)
        val parcel = Parcel.obtain()
        try {
            parcel.writeBundle(bundle(chosen)); parcel.setDataPosition(0)
            assertEquals(chosen, relaySettingsSaver(base).restore(requireNotNull(parcel.readBundle(javaClass.classLoader))))
        } finally { parcel.recycle() }
        val old = bundle(base).apply { keySet().toList().filter { it.startsWith("experimental.") }.forEach { remove(it) } }
        assertEquals(base, relaySettingsSaver(base).restore(old))
    }

}
