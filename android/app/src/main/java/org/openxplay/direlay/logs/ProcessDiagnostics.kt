// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-29
package org.openxplay.direlay.logs

import android.app.ActivityManager
import android.app.usage.UsageStatsManager
import android.content.Context
import android.os.Build
import android.os.PowerManager
import java.io.File
import java.time.Instant
import java.util.Properties

/** How Android treats this process right now. Each read may fail on some systems and then stays unknown. */
data class PowerState(val batteryExempt: Boolean? = null, val backgroundRestricted: Boolean? = null,
    val standbyBucket: Int? = null, val powerSave: Boolean? = null, val idle: Boolean? = null) {
    fun line() = "battery_exempt=${known(batteryExempt)} background_restricted=${known(backgroundRestricted)} " +
        "standby_bucket=${standbyBucket?.let(::standbyBucketName) ?: "unknown"} power_save=${known(powerSave)} idle=${known(idle)}"
}

fun readPowerState(context: Context): PowerState {
    val power = context.getSystemService(PowerManager::class.java)
    val activity = context.getSystemService(ActivityManager::class.java)
    return PowerState(
        runCatching { power?.isIgnoringBatteryOptimizations(context.packageName) }.getOrNull(),
        if (Build.VERSION.SDK_INT >= 28) runCatching { activity?.isBackgroundRestricted }.getOrNull() else null,
        if (Build.VERSION.SDK_INT >= 28) runCatching { context.getSystemService(UsageStatsManager::class.java)?.appStandbyBucket }.getOrNull() else null,
        runCatching { power?.isPowerSaveMode }.getOrNull(),
        runCatching { power?.isDeviceIdleMode }.getOrNull())
}

/** The process importance Android assigns now: foreground, visible, service, cached... */
fun processImportance(): String = runCatching {
    val info = ActivityManager.RunningAppProcessInfo()
    ActivityManager.getMyMemoryState(info)
    importanceName(info.importance)
}.getOrDefault("unknown")

/** One earlier end of this app's process, as Android 11+ recorded it. */
data class ProcessExit(val pid: Int, val atMillis: Long, val reason: String, val status: Int, val importance: String,
    val pssKb: Long, val rssKb: Long, val description: String?) {
    fun line() = "pid=$pid at=${Instant.ofEpochMilli(atMillis)} reason=$reason status=$status importance=$importance " +
        "pss_kb=$pssKb rss_kb=$rssKb" + (description?.takeIf { it.isNotBlank() }?.let { " description=${it.replace('\n', ' ')}" } ?: "")
}

/** Newest first; empty before Android 11 or when the system refuses. */
fun readProcessExits(context: Context, max: Int = 5): List<ProcessExit> {
    if (Build.VERSION.SDK_INT < 30) return emptyList()
    return runCatching {
        context.getSystemService(ActivityManager::class.java).getHistoricalProcessExitReasons(context.packageName, 0, max)
            .map { ProcessExit(it.pid, it.timestamp, exitReasonName(it.reason), it.status, importanceName(it.importance),
                it.pss, it.rss, it.description) }
    }.getOrDefault(emptyList())
}

/** The last known state of a process, kept on disk so the next process can tell how the previous one ended. */
class HeartbeatFile(private val file: File) {
    fun read(): Map<String, String>? = runCatching {
        if (!file.isFile) return null
        Properties().apply { file.inputStream().use(::load) }.let { values -> values.stringPropertyNames().associateWith { values.getProperty(it) } }
    }.getOrNull()

    fun write(values: Map<String, String>) {
        runCatching {
            file.parentFile?.mkdirs()
            val temporary = File(file.parentFile, "${file.name}.tmp")
            temporary.outputStream().use { output -> Properties().apply { putAll(values) }.store(output, null) }
            if (!temporary.renameTo(file)) { file.delete(); temporary.renameTo(file) }
        }
    }
}

internal fun exitReasonName(reason: Int) = when (reason) {
    1 -> "exit_self"; 2 -> "signaled"; 3 -> "low_memory"; 4 -> "crash"; 5 -> "crash_native"; 6 -> "anr"
    7 -> "initialization_failure"; 8 -> "permission_change"; 9 -> "excessive_resource_usage"; 10 -> "user_requested"
    11 -> "user_stopped"; 12 -> "dependency_died"; 13 -> "other"; 14 -> "freezer"; 15 -> "package_state_change"
    16 -> "package_updated"; else -> "unknown($reason)"
}

internal fun importanceName(importance: Int) = when (importance) {
    100 -> "foreground"; 125 -> "foreground_service"; 150 -> "top_sleeping_pre_28"; 200 -> "visible"; 230 -> "perceptible"
    300 -> "service"; 325 -> "top_sleeping"; 350 -> "cant_save_state"; 400 -> "cached"; 1000 -> "gone"
    else -> "importance_$importance"
}

internal fun standbyBucketName(bucket: Int) = when (bucket) {
    5 -> "exempted"; 10 -> "active"; 20 -> "working_set"; 30 -> "frequent"; 40 -> "rare"; 45 -> "restricted"; 50 -> "never"
    else -> "bucket_$bucket"
}

private fun known(value: Boolean?) = value?.toString() ?: "unknown"
