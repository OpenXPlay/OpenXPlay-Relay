// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-11-05
package org.openxplay.direlay.carlife

import android.content.Intent
import android.hardware.usb.UsbManager
import androidx.activity.ComponentActivity
import androidx.core.util.Consumer
import androidx.lifecycle.DefaultLifecycleObserver
import androidx.lifecycle.LifecycleOwner

/** AOA attachment is delivered to the Activity; only detachment is a system broadcast. */
internal fun ComponentActivity.observeCarlifeUsb(refresh: () -> Unit) {
    val attached = Consumer<Intent> { intent ->
        if (intent.action == UsbManager.ACTION_USB_ACCESSORY_ATTACHED) refresh()
    }
    addOnNewIntentListener(attached)
    lifecycle.addObserver(object : DefaultLifecycleObserver {
        override fun onResume(owner: LifecycleOwner) = refresh()
        override fun onDestroy(owner: LifecycleOwner) {
            removeOnNewIntentListener(attached)
        }
    })
}
