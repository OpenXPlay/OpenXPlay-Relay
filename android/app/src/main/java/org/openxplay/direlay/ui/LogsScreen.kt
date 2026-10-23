// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-22
package org.openxplay.direlay.ui

import android.graphics.Bitmap
import android.text.format.Formatter
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.Image
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.selection.selectableGroup
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.FilterQuality
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.platform.LocalClipboardManager
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalResources
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.DialogProperties
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import org.openxplay.direlay.R
import org.openxplay.direlay.logs.*
import java.io.File

private enum class LogsStep { PERIOD, COMPRESS, READY, SAVE, UPLOAD, QR, RESULT }

@Composable fun LogsScreen(files: LogFiles, uploader: LogUploader) {
    val context = LocalContext.current
    val resources = LocalResources.current
    val clipboard = LocalClipboardManager.current
    val scope = rememberCoroutineScope()
    var step by remember { mutableStateOf<LogsStep?>(null) }
    var range by remember { mutableStateOf(LogRange.LAST_10_MINUTES) }
    var file by remember { mutableStateOf<File?>(null) }
    var job by remember { mutableStateOf<Job?>(null) }
    var message by remember { mutableStateOf("") }
    var uploadError by remember { mutableStateOf(false) }
    var link by remember { mutableStateOf("") }
    var qr by remember { mutableStateOf<ImageBitmap?>(null) }
    var copied by remember { mutableStateOf(false) }
    fun result(text: String, back: Boolean = false) { message = text; uploadError = back; step = LogsStep.RESULT }
    val create = rememberLauncherForActivityResult(ActivityResultContracts.CreateDocument("application/zip")) { uri ->
        val archive = file
        if (uri != null && archive != null) {
            step = LogsStep.SAVE
            job = scope.launch {
                try { files.save(archive, uri); result(resources.getString(R.string.logs_saved)) }
                catch (cancelled: CancellationException) { throw cancelled }
                catch (failure: Exception) { result(files.failureMessage(failure).ifBlank { resources.getString(R.string.logs_io_error) }) }
            }
        }
    }
    Card(Modifier.fillMaxWidth()) {
        Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            Text(stringResource(R.string.logs_description))
            TextButton(enabled = step == null, onClick = { range = LogRange.LAST_10_MINUTES; step = LogsStep.PERIOD }) {
                Text(stringResource(R.string.logs_export))
            }
        }
    }
    when (step) {
        LogsStep.PERIOD -> AlertDialog(onDismissRequest = { step = null },
            title = { Text(stringResource(R.string.logs_choose_period)) },
            text = { Column(Modifier.selectableGroup()) {
                LogRange.entries.forEach { option ->
                    Row(Modifier.fillMaxWidth().selectable(range == option, role = Role.RadioButton, onClick = { range = option })
                        .padding(vertical = 8.dp), verticalAlignment = Alignment.CenterVertically) {
                        RadioButton(range == option, null)
                        Text(stringResource(logRangeTitle(option)), Modifier.padding(start = 8.dp))
                    }
                }
            } }, confirmButton = { TextButton(onClick = {
                step = LogsStep.COMPRESS
                file = null
                job = scope.launch {
                    try {
                        file = files.export(range)
                        if (file == null) result(resources.getString(R.string.logs_empty)) else step = LogsStep.READY
                    } catch (cancelled: CancellationException) { throw cancelled }
                    catch (_: Exception) { result(resources.getString(R.string.logs_io_error)) }
                }
            }) { Text(stringResource(R.string.logs_export_confirm)) } },
            dismissButton = { TextButton(onClick = { step = null }) { Text(stringResource(android.R.string.cancel)) } })
        LogsStep.COMPRESS, LogsStep.UPLOAD, LogsStep.SAVE -> {
            val title = when (step) {
                LogsStep.COMPRESS -> R.string.logs_compressing
                LogsStep.UPLOAD -> R.string.logs_uploading
                else -> R.string.logs_saving
            }
            AlertDialog(onDismissRequest = {}, properties = DialogProperties(dismissOnBackPress = false, dismissOnClickOutside = false),
                title = { Text(stringResource(title)) },
                text = { Box(Modifier.fillMaxWidth(), contentAlignment = Alignment.Center) { CircularProgressIndicator() } },
                confirmButton = { TextButton(onClick = {
                    job?.cancel()
                    step = if (step == LogsStep.UPLOAD || step == LogsStep.SAVE) LogsStep.READY else null
                }) { Text(stringResource(android.R.string.cancel)) } })
        }
        LogsStep.READY -> file?.let { archive ->
            AlertDialog(onDismissRequest = { step = null }, title = { Text(stringResource(R.string.logs_ready)) },
                text = { Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
                    Text("${archive.name}\n${Formatter.formatFileSize(context, archive.length())}")
                    Text(stringResource(R.string.logs_upload_hint), style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant)
                } },
                confirmButton = { Column(Modifier.fillMaxWidth(), horizontalAlignment = Alignment.End) {
                    TextButton(onClick = { create.launch(archive.name) }) { Text(stringResource(R.string.logs_save)) }
                    TextButton(onClick = {
                        step = LogsStep.UPLOAD
                        job = scope.launch {
                            try {
                                val uploaded = uploader.upload(archive, archive.name)
                                qr = withContext(Dispatchers.Default) {
                                    val matrix = LogQrCode.matrix(uploaded.link)
                                    val pixels = IntArray(matrix.width * matrix.height) { index ->
                                        if (matrix[index % matrix.width, index / matrix.width]) android.graphics.Color.BLACK else android.graphics.Color.WHITE
                                    }
                                    Bitmap.createBitmap(pixels, matrix.width, matrix.height, Bitmap.Config.ARGB_8888).asImageBitmap()
                                }
                                link = uploaded.link; copied = false; step = LogsStep.QR
                            } catch (cancelled: CancellationException) { throw cancelled }
                            catch (failure: Exception) {
                                val text = when ((failure as? LogUploadFailure)?.reason) {
                                    "no-internet" -> resources.getString(R.string.logs_no_internet)
                                    "too-large" -> resources.getString(R.string.logs_too_large)
                                    "http", "invalid-response" -> resources.getString(R.string.logs_upload_http, (failure as? LogUploadFailure)?.status ?: 0)
                                    else -> resources.getString(R.string.logs_upload_error)
                                }
                                result(text, back = true)
                            }
                        }
                    }) { Text(stringResource(R.string.logs_upload)) }
                    TextButton(onClick = { step = null }) { Text(stringResource(android.R.string.cancel)) }
                } })
        }
        LogsStep.QR -> AlertDialog(onDismissRequest = { step = null }, title = { Text(stringResource(R.string.logs_qr_title)) },
            text = { Column(Modifier.verticalScroll(rememberScrollState()), verticalArrangement = Arrangement.spacedBy(12.dp),
                horizontalAlignment = Alignment.CenterHorizontally) {
                qr?.let { Image(it, stringResource(R.string.logs_qr_description), Modifier.size(240.dp), filterQuality = FilterQuality.None) }
                Text(stringResource(R.string.logs_scan))
                SelectionContainer { Text(link) }
                if (copied) Text(stringResource(R.string.logs_copied))
            } }, confirmButton = { TextButton(onClick = { step = null }) { Text(stringResource(R.string.logs_done)) } },
            dismissButton = { TextButton(onClick = { clipboard.setText(AnnotatedString(link)); copied = true }) {
                Text(stringResource(R.string.logs_copy))
            } })
        LogsStep.RESULT -> AlertDialog(onDismissRequest = { step = null }, title = { Text(stringResource(R.string.logs_title)) },
            text = { Text(message) }, confirmButton = { TextButton(onClick = { step = null }) { Text(stringResource(android.R.string.ok)) } },
            dismissButton = if (uploadError) ({ TextButton(onClick = { step = LogsStep.READY }) { Text(stringResource(R.string.ui_back)) } }) else null)
        null -> Unit
    }
}

fun logRangeTitle(range: LogRange) = when (range) {
    LogRange.LAST_10_MINUTES -> R.string.logs_10_minutes
    LogRange.LAST_30_MINUTES -> R.string.logs_30_minutes
    LogRange.LAST_24_HOURS -> R.string.logs_24_hours
}
