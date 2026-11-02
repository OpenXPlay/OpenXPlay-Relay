// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-11-02
package org.openxplay.direlay.carlife

import android.app.PendingIntent
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.hardware.usb.UsbAccessory
import android.hardware.usb.UsbManager
import android.os.Build
import android.os.ParcelFileDescriptor
import org.openxplay.stream.carlife.CarlifeOutput
import java.io.BufferedInputStream
import java.io.Closeable
import java.io.InputStream
import java.util.concurrent.Executors
import java.util.concurrent.RejectedExecutionException

/** Android supplies the accessory role; Stream owns all CarLife bytes and channel multiplexing. */
class AndroidCarlifeAccessory(
    context: Context,
    private val output: CarlifeOutput,
    private val report: (String) -> Unit = {},
) : Closeable {
    private val context = context.applicationContext
    private val manager = this.context.getSystemService(UsbManager::class.java)
    private val worker = Executors.newSingleThreadExecutor { task ->
        Thread(task, "carlife-accessory").apply { isDaemon = true }
    }
    private val lifecycle = AccessoryAttachment<UsbAccessory>()
    private val permissionAction = "${this.context.packageName}.CARLIFE_USB_PERMISSION"
    private var registered = false
    private var permission: PendingIntent? = null
    private var requested: UsbAccessory? = null
    private val receiver = object : BroadcastReceiver() {
        override fun onReceive(context: Context, intent: Intent) {
            // Read the current system inventory and permission, never trust broadcast extras.
            if (intent.action == permissionAction || intent.action == UsbManager.ACTION_USB_ACCESSORY_ATTACHED ||
                intent.action == UsbManager.ACTION_USB_ACCESSORY_DETACHED) refresh()
        }
    }

    /** Call after attaching the output to its hub. No USB I/O runs on the caller's thread. */
    @Synchronized
    fun start() {
        if (registered || lifecycle.closed) return
        val filter = IntentFilter().apply {
            addAction(permissionAction)
            addAction(UsbManager.ACTION_USB_ACCESSORY_ATTACHED)
            addAction(UsbManager.ACTION_USB_ACCESSORY_DETACHED)
        }
        if (Build.VERSION.SDK_INT >= 33) context.registerReceiver(receiver, filter, Context.RECEIVER_NOT_EXPORTED)
        else @Suppress("DEPRECATION") context.registerReceiver(receiver, filter)
        registered = true
        permission = PendingIntent.getBroadcast(context, 0,
            Intent(permissionAction).setPackage(context.packageName),
            PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT)
        refresh()
    }

    private fun refresh() {
        if (lifecycle.closed) return
        try {
            worker.execute {
                if (lifecycle.closed) return@execute
                runCatching { reconcile() }.onFailure { failure ->
                    report("usb_error:${failure.javaClass.simpleName}")
                }
            }
        } catch (_: RejectedExecutionException) {
            // A detach or permission broadcast can race shutdown.
        }
    }

    private fun reconcile() {
        val usb = manager ?: return
        val accessory = usb.accessoryList?.firstOrNull {
            carlifeAccessoryMatches(it.manufacturer, it.model)
        }
        if (accessory == null) {
            requested = null
            if (lifecycle.clear()) report("usb_detached")
            return
        }
        if (lifecycle.contains(accessory)) return
        lifecycle.clear()
        if (!usb.hasPermission(accessory)) {
            if (requested != accessory) {
                requested = accessory
                permission?.let { usb.requestPermission(accessory, it) }
                report("usb_permission")
            }
            return
        }
        val generation = lifecycle.generation()
        val descriptor = usb.openAccessory(accessory) ?: run {
            report("usb_unavailable")
            return
        }
        val streams = try { AccessoryStreams(descriptor) }
            catch (failure: Exception) { descriptor.close(); throw failure }
        try {
            if (!lifecycle.isCurrent(generation)) {
                streams.close()
                return
            }
            val attachment = output.attachAccessory(streams.input, streams.output, streams)
            val lease = Closeable {
                runCatching { attachment.close() }
                streams.close()
            }
            if (lifecycle.install(accessory, generation, lease)) report("usb_connected")
        } catch (failure: Exception) {
            streams.close()
            throw failure
        }
    }

    @Synchronized
    override fun close() {
        lifecycle.close()
        if (registered) runCatching { context.unregisterReceiver(receiver) }
        registered = false
        permission?.cancel()
        permission = null
        worker.shutdownNow()
    }
}

internal fun carlifeAccessoryMatches(manufacturer: String?, model: String?): Boolean =
    manufacturer == "Baidu" && model == "CarLife"

// Android drops a transfer's unread remainder; framing must consume a buffered byte stream.
internal fun bufferAccessoryInput(input: InputStream): InputStream = BufferedInputStream(input, 16_384)

/** Each direction owns a duplicate, so closing a stream cannot close a reused descriptor number. */
private class AccessoryStreams(private val descriptor: ParcelFileDescriptor) : Closeable {
    val input = bufferAccessoryInput(
        ParcelFileDescriptor.AutoCloseInputStream(ParcelFileDescriptor.dup(descriptor.fileDescriptor)))
    val output = try {
        ParcelFileDescriptor.AutoCloseOutputStream(ParcelFileDescriptor.dup(descriptor.fileDescriptor))
    } catch (failure: Exception) {
        input.close()
        throw failure
    }
    private var closed = false
    @Synchronized
    override fun close() {
        if (closed) return
        closed = true
        runCatching { descriptor.close() }
        runCatching { input.close() }
        runCatching { output.close() }
    }
}
