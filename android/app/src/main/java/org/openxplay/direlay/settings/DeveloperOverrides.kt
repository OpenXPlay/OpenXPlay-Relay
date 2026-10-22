// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-21
package org.openxplay.direlay.settings

import org.openxplay.stream.clink.ClinkHevcMode
import org.openxplay.stream.clink.ClinkPictureSize

data class DeveloperOverrides(
    val active: Boolean = true,
    val carlinkTrace: Boolean = false,
    val carlinkHevcEnabled: Boolean = false,
    val carlinkHevc: ClinkHevcMode = ClinkHevcMode.STREAM_ONLY,
    val carlinkPictureSizeEnabled: Boolean = false,
    val carlinkPictureSize: ClinkPictureSize = ClinkPictureSize.WINDOW,
    val physicalWidthEnabled: Boolean = false,
    val physicalWidthOverrideMm: Int = 0,
    val fpsEnabled: Boolean = false,
    val fps: Int = 30,
    val localPreview: Boolean = false,
    val playCarPlayAudio: Boolean = false,
    val microphoneSourceEnabled: Boolean = false,
    val microphoneSource: MicrophoneSource = MicrophoneSource.HEAD_UNIT,
    val webPortEnabled: Boolean = false,
    val webPort: Int? = null,
    val carlinkDeviceModelEnabled: Boolean = false,
    val carlinkDeviceModel: String = "Android relay",
) {
    fun effectiveSettings(base: RelaySettings, developerMode: Boolean): RelaySettings {
        val enabled = developerMode && active
        return base.copy(
            carlinkTrace = enabled && carlinkTrace,
            carlinkHevc = if (enabled && carlinkHevcEnabled) carlinkHevc else ClinkHevcMode.STREAM_ONLY,
            carlinkPictureSize = if (enabled && carlinkPictureSizeEnabled) carlinkPictureSize else ClinkPictureSize.WINDOW,
            fps = if (enabled && fpsEnabled) fps else 30,
            localPreview = enabled && localPreview,
            playCarPlayAudio = enabled && playCarPlayAudio,
            microphoneSource = if (enabled && microphoneSourceEnabled) microphoneSource else MicrophoneSource.HEAD_UNIT,
            webPort = if (enabled && webPortEnabled) webPort else null,
            carlinkDeviceModel = if (enabled && carlinkDeviceModelEnabled) carlinkDeviceModel else "Android relay",
        )
    }

    fun effectivePhysicalWidth(developerMode: Boolean): Int =
        if (developerMode && active && physicalWidthEnabled) physicalWidthOverrideMm else 0

    fun effectiveSampleVehicleData(requested: Boolean, developerMode: Boolean): Boolean = requested && developerMode && active

    internal fun remember(value: RelaySettings, developerMode: Boolean): DeveloperOverrides {
        if (!developerMode || !active) return this
        return copy(
            carlinkTrace = carlinkTrace && value.carlinkTrace,
            carlinkHevc = if (carlinkHevcEnabled) value.carlinkHevc else carlinkHevc,
            carlinkPictureSize = if (carlinkPictureSizeEnabled) value.carlinkPictureSize else carlinkPictureSize,
            fps = if (fpsEnabled) value.fps else fps,
            localPreview = localPreview && value.localPreview,
            playCarPlayAudio = playCarPlayAudio && value.playCarPlayAudio,
            microphoneSource = if (microphoneSourceEnabled) value.microphoneSource else microphoneSource,
            webPort = if (webPortEnabled) value.webPort else webPort,
            carlinkDeviceModel = if (carlinkDeviceModelEnabled) value.carlinkDeviceModel else carlinkDeviceModel,
        )
    }

    companion object {
        internal fun migrate(value: RelaySettings, physicalWidth: Int) = DeveloperOverrides(
            carlinkTrace = value.carlinkTrace,
            carlinkHevcEnabled = value.carlinkHevc != ClinkHevcMode.STREAM_ONLY,
            carlinkHevc = value.carlinkHevc,
            carlinkPictureSizeEnabled = value.carlinkPictureSize != ClinkPictureSize.WINDOW,
            carlinkPictureSize = value.carlinkPictureSize,
            physicalWidthEnabled = physicalWidth != 0,
            physicalWidthOverrideMm = physicalWidth,
            fpsEnabled = value.fps != 30,
            fps = value.fps,
            localPreview = value.localPreview,
            playCarPlayAudio = value.playCarPlayAudio,
            microphoneSourceEnabled = value.microphoneSource != MicrophoneSource.HEAD_UNIT,
            microphoneSource = value.microphoneSource,
            webPortEnabled = value.webPort != null,
            webPort = value.webPort,
            carlinkDeviceModelEnabled = value.carlinkDeviceModel != "Android relay",
            carlinkDeviceModel = value.carlinkDeviceModel,
        )
    }
}
