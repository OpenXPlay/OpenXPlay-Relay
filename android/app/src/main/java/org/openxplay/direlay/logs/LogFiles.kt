// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-22
package org.openxplay.direlay.logs

import android.content.Context
import android.content.pm.ApplicationInfo
import android.net.Uri
import android.os.Build
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.ensureActive
import kotlinx.coroutines.withContext
import java.io.File

class LogFiles(private val context: Context, secrets: LogSecrets, traceDirectory: () -> File? = { null }) {
    /** Facts other components add to the export summary; must not block for long. */
    @Volatile var details: () -> String = { "" }
    private val redactor = LogRedactor(secrets)
    private val store = LogSegmentStore(File(context.noBackupFilesDir, "logs"))
    val recorder = LogRecorder(store, redactor)
    private val exporter = LogExporter(File(context.cacheDir, "log-exports"), store, ::metadata,
        flush = recorder::flush, traceDirectory = traceDirectory)
    suspend fun export(range: LogRange) = exporter.export(range)
    fun failureMessage(failure: Exception): String = runCatching {
        failure.message.orEmpty().lineSequence().take(4).joinToString("\n") { redactor.redact(it) }
    }.getOrDefault("")
    suspend fun save(file: File, uri: Uri) = withContext(Dispatchers.IO) {
        val job = currentCoroutineContext()
        checkNotNull(context.contentResolver.openOutputStream(uri, "wt")).use { output ->
            file.inputStream().use { input ->
                val buffer = ByteArray(8192)
                while (true) {
                    job.ensureActive()
                    val size = input.read(buffer)
                    if (size < 0) break
                    output.write(buffer, 0, size)
                }
            }
        }
    }
    fun start() {
        val info = context.packageManager.getPackageInfo(context.packageName, 0)
        recorder.start(info.versionName.orEmpty(), versionCode(info))
    }
    private fun metadata(): String {
        val info = context.packageManager.getPackageInfo(context.packageName, 0)
        return "App: OpenXPlay Relay\nPackage: ${context.packageName}\nVersion: ${info.versionName} (${versionCode(info)})\n" +
            "Build type: ${if (context.applicationInfo.flags and ApplicationInfo.FLAG_DEBUGGABLE != 0) "debug" else "release"}\n" +
            "Android: ${Build.VERSION.RELEASE} (SDK ${Build.VERSION.SDK_INT})\nManufacturer: ${Build.MANUFACTURER}\n" +
            "Model: ${Build.MODEL}\nBuild display: ${Build.DISPLAY}\n" + recorder.summary() + "\n" +
            runCatching { details() }.getOrDefault("")
    }
    @Suppress("DEPRECATION") private fun versionCode(info: android.content.pm.PackageInfo): Long =
        if (Build.VERSION.SDK_INT >= 28) info.longVersionCode else info.versionCode.toLong()
}
