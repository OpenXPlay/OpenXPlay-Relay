// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-22
package org.openxplay.direlay.ui

import android.graphics.Bitmap
import android.os.SystemClock
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
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.DialogProperties
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import org.openxplay.direlay.ui.design.*
import org.openxplay.direlay.R
import org.openxplay.direlay.logs.*
import java.io.File
import java.time.Instant
import java.util.Date

private enum class LogsStep { PERIOD, COMPRESS, READY, SAVE, UPLOAD, PREPARE_QR, QR, RESULT }

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
    var receipt by remember(uploader) { mutableStateOf(uploader.latestReceipt()) }
    var qr by remember { mutableStateOf<ImageBitmap?>(null) }
    var copied by remember { mutableStateOf(false) }
    var uploadProgress by remember { mutableStateOf<LogUploadProgress>(LogUploadProgress.Connecting) }
    fun result(text: String, back: Boolean = false) { message = text; uploadError = back; step = LogsStep.RESULT }
    suspend fun showReceipt(uploaded: UploadResult) {
        step = LogsStep.PREPARE_QR
        receipt = uploaded
        qr = null
        copied = false
        try {
            qr = withContext(Dispatchers.Default) {
                val matrix = LogQrCode.matrix(uploaded.link)
                val pixels = IntArray(matrix.width * matrix.height) { index ->
                    if (matrix[index % matrix.width, index / matrix.width]) android.graphics.Color.BLACK else android.graphics.Color.WHITE
                }
                Bitmap.createBitmap(pixels, matrix.width, matrix.height, Bitmap.Config.ARGB_8888).asImageBitmap()
            }
        } catch (cancelled: CancellationException) { throw cancelled }
        catch (_: Exception) { /* The confirmed receipt remains usable without its QR image. */ }
        step = LogsStep.QR
    }
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
    InsetGroup(footer = stringResource(R.string.logs_description)) {
        InsetRow(R.drawable.symbol_code, IconHue.LOGS, stringResource(R.string.logs_export),
            onClick = if (step == null) ({ range = LogRange.LAST_10_MINUTES; step = LogsStep.PERIOD }) else null)
        receipt?.let { latest ->
            InsetDivider()
            InsetRow(R.drawable.symbol_code, IconHue.LOGS, stringResource(R.string.logs_latest_receipt),
                onClick = if (step == null) ({
                    step = LogsStep.PREPARE_QR
                    job = scope.launch { showReceipt(latest) }
                }) else null)
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
        LogsStep.COMPRESS, LogsStep.UPLOAD, LogsStep.SAVE, LogsStep.PREPARE_QR -> key(step) {
            val title = when (step) {
                LogsStep.COMPRESS -> R.string.logs_compressing
                LogsStep.UPLOAD -> when (uploadProgress) {
                    LogUploadProgress.Connecting -> R.string.logs_connecting
                    LogUploadProgress.AwaitingReceipt -> R.string.logs_confirming
                    is LogUploadProgress.Sending -> R.string.logs_uploading
                }
                LogsStep.PREPARE_QR -> R.string.logs_preparing_receipt
                else -> R.string.logs_saving
            }
            val detail = when (step) {
                LogsStep.COMPRESS -> null
                LogsStep.UPLOAD -> when (uploadProgress) {
                    LogUploadProgress.Connecting -> R.string.logs_connecting_detail
                    LogUploadProgress.AwaitingReceipt -> R.string.logs_confirming_detail
                    is LogUploadProgress.Sending -> R.string.logs_sending_detail
                }
                LogsStep.PREPARE_QR -> R.string.logs_preparing_receipt_detail
                else -> R.string.logs_saving_detail
            }
            LogsProgressDialog(title, detail, if (step == LogsStep.UPLOAD) uploadProgress as? LogUploadProgress.Sending else null,
                waitingForUpload = step == LogsStep.UPLOAD, onCancel = {
                    job?.cancel()
                    when (step) {
                        LogsStep.UPLOAD -> result(resources.getString(R.string.logs_upload_cancelled), back = true)
                        LogsStep.SAVE -> step = LogsStep.READY
                        else -> step = null
                    }
                })
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
                        uploadProgress = LogUploadProgress.Connecting
                        step = LogsStep.UPLOAD
                        job = scope.launch {
                            try {
                                val uploaded = uploader.upload(archive, archive.name) { progress ->
                                    withContext(Dispatchers.Main.immediate) { uploadProgress = progress }
                                }
                                showReceipt(uploaded)
                            } catch (cancelled: CancellationException) {
                                receipt = uploader.latestReceipt() ?: receipt
                                throw cancelled
                            }
                            catch (failure: Exception) {
                                val uploadFailure = failure as? LogUploadFailure
                                val text = when (uploadFailure?.reason) {
                                    "no-internet" -> resources.getString(R.string.logs_no_internet)
                                    "too-large" -> resources.getString(R.string.logs_too_large)
                                    "empty" -> resources.getString(R.string.logs_empty)
                                    "http" -> resources.getString(R.string.logs_upload_http, uploadFailure.status ?: 0)
                                    else -> resources.getString(R.string.logs_upload_error)
                                }
                                val wait = uploadFailure?.retryAfterSeconds?.let { "\n" + resources.getString(R.string.logs_retry_after, it) }.orEmpty()
                                result(text + wait, back = true)
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
                    ?: Text(stringResource(R.string.logs_qr_unavailable))
                Text(stringResource(R.string.logs_scan))
                receipt?.let { uploaded ->
                    SelectionContainer { Text(stringResource(R.string.logs_receipt_id, uploaded.id)) }
                    val expiry = Date.from(Instant.parse(uploaded.expiresAt))
                    Text(stringResource(R.string.logs_receipt_expires,
                        android.text.format.DateFormat.getDateFormat(context).format(expiry) + " " +
                            android.text.format.DateFormat.getTimeFormat(context).format(expiry)))
                    SelectionContainer { Text(uploaded.link) }
                }
                if (copied) Text(stringResource(R.string.logs_copied))
            } }, confirmButton = { TextButton(onClick = { step = null }) { Text(stringResource(R.string.logs_done)) } },
            dismissButton = { TextButton(onClick = { receipt?.let { clipboard.setText(AnnotatedString(it.link)); copied = true } }) {
                Text(stringResource(R.string.logs_copy))
            } })
        LogsStep.RESULT -> AlertDialog(onDismissRequest = { step = null }, title = { Text(stringResource(R.string.logs_title)) },
            text = { Text(message) }, confirmButton = { TextButton(onClick = { step = null }) { Text(stringResource(android.R.string.ok)) } },
            dismissButton = if (uploadError) ({ TextButton(onClick = { step = LogsStep.READY }) { Text(stringResource(R.string.ui_back)) } }) else null)
        null -> Unit
    }
}

@Composable private fun LogsProgressDialog(title: Int, detail: Int?, transfer: LogUploadProgress.Sending?,
    waitingForUpload: Boolean, onCancel: () -> Unit) {
    val context = LocalContext.current
    val started = remember { SystemClock.elapsedRealtime() }
    var elapsed by remember { mutableLongStateOf(0L) }
    LaunchedEffect(Unit) {
        while (true) {
            delay(1000)
            elapsed = (SystemClock.elapsedRealtime() - started) / 1000
        }
    }
    AlertDialog(onDismissRequest = {}, properties = DialogProperties(dismissOnBackPress = false, dismissOnClickOutside = false),
        title = { Text(stringResource(title), Modifier.semantics { liveRegion = LiveRegionMode.Polite }) },
        text = { Column(Modifier.fillMaxWidth().verticalScroll(rememberScrollState()), verticalArrangement = Arrangement.spacedBy(16.dp)) {
            if (transfer != null) {
                val fraction = (transfer.bytesSent.toFloat() / transfer.totalBytes).coerceIn(0f, 1f)
                LinearProgressIndicator(progress = { fraction }, modifier = Modifier.fillMaxWidth())
                Text(stringResource(R.string.logs_bytes_sent, (transfer.bytesSent * 100 / transfer.totalBytes).toInt(),
                    Formatter.formatFileSize(context, transfer.bytesSent), Formatter.formatFileSize(context, transfer.totalBytes)))
            } else {
                Box(Modifier.fillMaxWidth(), contentAlignment = Alignment.Center) { CircularProgressIndicator() }
            }
            if (detail != null) Text(stringResource(detail))
            Text(stringResource(R.string.logs_elapsed, elapsed / 60, elapsed % 60),
                style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
            if (waitingForUpload && elapsed >= 15) Text(stringResource(R.string.logs_upload_wait), style = MaterialTheme.typography.bodySmall)
        } },
        confirmButton = { TextButton(onClick = onCancel) { Text(stringResource(android.R.string.cancel)) } })
}

fun logRangeTitle(range: LogRange) = when (range) {
    LogRange.LAST_10_MINUTES -> R.string.logs_10_minutes
    LogRange.LAST_30_MINUTES -> R.string.logs_30_minutes
    LogRange.LAST_24_HOURS -> R.string.logs_24_hours
}
