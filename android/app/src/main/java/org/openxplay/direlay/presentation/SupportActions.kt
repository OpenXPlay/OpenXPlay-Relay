// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-25
package org.openxplay.direlay.presentation

import android.net.Uri
import kotlinx.coroutines.flow.StateFlow
import org.openxplay.direlay.config.ConfigFileResult
import org.openxplay.direlay.logs.*
import org.openxplay.direlay.update.*
import java.io.File

class ConfigActions(val hasPrivateData: suspend () -> Boolean,
    val export: suspend (Uri, Boolean) -> ConfigFileResult, val import: suspend (Uri, Boolean) -> ConfigFileResult)
class LogActions(val export: suspend (LogRange) -> File?, val save: suspend (File, Uri) -> Unit,
    val failureMessage: suspend (Exception) -> String)
class UploadActions(val latestReceipt: suspend () -> UploadResult?,
    val upload: suspend (File, String, suspend (LogUploadProgress) -> Unit) -> UploadResult)
class UpdateActions(val state: StateFlow<UpdateState>, val check: () -> Unit, val setChannel: (UpdateChannel) -> Unit)
class SupportActions(val config: ConfigActions, val logs: LogActions, val upload: UploadActions, val updates: UpdateActions)
