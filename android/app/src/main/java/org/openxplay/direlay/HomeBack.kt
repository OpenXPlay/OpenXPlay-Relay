// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-11-04
package org.openxplay.direlay

import android.app.ActivityManager
import android.content.Intent
import androidx.activity.ComponentActivity
import androidx.activity.OnBackPressedCallback
import com.shilapi.xcertplay.CarPlayHostActivity
import com.shilapi.xcertplay.SameTaskCarPlayHostActivity

/**
 * The running page (DiPlay's CarPlay host) is a page above Home, but it lives in its own task under Relay's
 * when Home was opened from it. Back on Home would move Relay's task back and show the running page again,
 * so while that task exists Back on Home leaves for the launcher. Pages and the drawer handle Back first.
 */
internal class HomeBack(private val activity: ComponentActivity) : OnBackPressedCallback(false) {
    /** Call when Home comes to the front; the running page's task cannot appear while Home is in front. */
    fun refresh() {
        isEnabled = runningPageBehind(runCatching {
            activity.getSystemService(ActivityManager::class.java)?.appTasks.orEmpty()
                .map { task -> runCatching { task.taskInfo?.baseActivity?.className }.getOrNull() }
        }.getOrDefault(emptyList()))
    }

    override fun handleOnBackPressed() {
        val launcher = Intent(Intent.ACTION_MAIN).addCategory(Intent.CATEGORY_HOME).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
        if (runCatching { activity.startActivity(launcher) }.isFailure) activity.moveTaskToBack(true)
    }

    companion object {
        private val hosts = setOf(CarPlayHostActivity::class.java.name, SameTaskCarPlayHostActivity::class.java.name)

        fun runningPageBehind(baseActivities: List<String?>): Boolean = baseActivities.any { it in hosts }
    }
}
