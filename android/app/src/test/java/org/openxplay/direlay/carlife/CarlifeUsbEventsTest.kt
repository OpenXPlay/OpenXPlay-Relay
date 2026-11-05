// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-11-05
package org.openxplay.direlay.carlife

import android.app.Application
import android.app.PendingIntent
import android.content.Intent
import android.hardware.usb.UsbAccessory
import android.hardware.usb.UsbManager
import android.os.Looper
import android.os.ParcelFileDescriptor
import androidx.activity.ComponentActivity
import java.io.Closeable
import java.util.concurrent.CopyOnWriteArrayList
import java.util.concurrent.ExecutorService
import java.util.concurrent.TimeUnit
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import org.openxplay.stream.carlife.CarlifeConfig
import org.openxplay.stream.carlife.CarlifeOutput
import org.openxplay.stream.core.*
import org.robolectric.Robolectric
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config
import org.robolectric.annotation.Implementation
import org.robolectric.annotation.Implements
import org.robolectric.shadow.api.Shadow
import org.robolectric.shadows.ShadowUsbManager
import org.robolectric.util.ReflectionHelpers
import org.robolectric.util.ReflectionHelpers.ClassParameter

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [30, 33], application = Application::class, shadows = [CarlifeUsbEventsTest.UsbShadow::class])
class CarlifeUsbEventsTest {
    @Implements(UsbManager::class)
    class UsbShadow : ShadowUsbManager() {
        var opens = 0
        var scans = 0
        var requests = 0
        var unavailable = false
        val ioThreads = mutableListOf<Thread>()

        @Implementation
        public override fun getAccessoryList(): Array<UsbAccessory>? {
            scans++
            ioThreads += Thread.currentThread()
            return super.getAccessoryList()
        }

        @Implementation
        public override fun openAccessory(accessory: UsbAccessory): ParcelFileDescriptor? {
            opens++
            ioThreads += Thread.currentThread()
            if (unavailable) return null
            // Robolectric's dup shadow requires a seekable file descriptor.
            return super.openAccessory(accessory)
        }

        @Implementation
        fun requestPermission(accessory: UsbAccessory, intent: PendingIntent) {
            requests++
            ioThreads += Thread.currentThread()
        }
    }

    private class Harness : Closeable {
        val context = RuntimeEnvironment.getApplication()
        val manager = context.getSystemService(UsbManager::class.java)
        val usb = Shadow.extract<UsbShadow>(manager)
        val reports = CopyOnWriteArrayList<String>()
        val output = CarlifeOutput(CarlifeConfig(tcp = false, discovery = false))
        val adapter = AndroidCarlifeAccessory(context, output, reports::add)
        val activity = Robolectric.buildActivity(ComponentActivity::class.java).setup()

        init {
            output.open(object : OutputHost {
                override fun capabilities(capabilities: OutputCapabilities) = Unit
                override fun control(event: ControlEvent) = Unit
                override fun stats(stats: OutputStats) = Unit
            })
            activity.get().observeCarlifeUsb(adapter::refresh)
        }

        fun start() { adapter.start(); drain() }
        fun attached() {
            activity.newIntent(Intent(UsbManager.ACTION_USB_ACCESSORY_ATTACHED))
            drain()
        }
        fun broadcast(action: String) {
            context.sendBroadcast(Intent(action).setPackage(context.packageName))
            shadowOf(Looper.getMainLooper()).idle()
            drain()
        }

        // A worker barrier makes inventory assertions deterministic without sleeps or polling.
        fun drain() {
            val worker = AndroidCarlifeAccessory::class.java.getDeclaredField("worker").apply { isAccessible = true }
                .get(adapter) as ExecutorService
            worker.submit {}.get(5, TimeUnit.SECONDS)
        }

        override fun close() {
            activity.pause().stop().destroy()
            adapter.close()
            output.close()
        }
    }

    private fun accessory(manufacturer: String = "Baidu", model: String = "CarLife") =
        ReflectionHelpers.callConstructor(UsbAccessory::class.java,
            ClassParameter.from(String::class.java, manufacturer),
            ClassParameter.from(String::class.java, model),
            ClassParameter.from(String::class.java, "private description"),
            ClassParameter.from(String::class.java, "1"),
            ClassParameter.from(String::class.java, "private URI"),
            ClassParameter.from(String::class.java, "private serial"))

    @Test fun attachIntentAfterStartupConnectsWithoutAnAttachedBroadcastAndReplugConnectsAgain() {
        Harness().use { h ->
            h.start()
            assertEquals(listOf("usb_scan state=absent count=0"), h.reports.toList())
            val car = accessory()
            h.usb.setAttachedUsbAccessory(car)
            h.usb.grantPermission(car)
            h.attached()
            assertEquals(1, h.usb.opens)
            assertEquals(h.reports.toString(), 1, h.reports.count { it == "usb_connected" })
            h.attached()
            assertEquals(1, h.usb.opens)

            h.usb.setAttachedUsbAccessory(null)
            h.broadcast(UsbManager.ACTION_USB_ACCESSORY_DETACHED)
            assertTrue(h.reports.contains("usb_detached"))
            h.usb.setAttachedUsbAccessory(car)
            h.attached()
            assertEquals(2, h.usb.opens)
            assertEquals(2, h.reports.count { it == "usb_connected" })
            assertTrue(h.usb.ioThreads.all { it !== Looper.getMainLooper().thread })
        }
    }

    @Test fun anAccessoryPresentBeforeOutputStartupIsStillDiscoveredAfterAnEarlyResume() {
        Harness().use { h ->
            val car = accessory()
            h.usb.setAttachedUsbAccessory(car)
            h.attached()
            assertEquals(0, h.usb.scans)
            h.start()
            assertEquals(1, h.usb.requests)
            assertEquals(0, h.usb.opens)
        }
    }

    @Test fun resumeRescansInventoryAndUnrelatedActivityIntentsCannotAttachForgedExtras() {
        Harness().use { h ->
            h.start()
            val scans = h.usb.scans
            val car = accessory()
            h.activity.newIntent(Intent(Intent.ACTION_VIEW).putExtra(UsbManager.EXTRA_ACCESSORY, car))
            h.drain()
            assertEquals(scans, h.usb.scans)
            assertEquals(0, h.usb.opens)

            h.usb.setAttachedUsbAccessory(car)
            h.usb.grantPermission(car)
            h.activity.pause().stop().start().resume()
            h.drain()
            assertEquals(1, h.usb.opens)
            assertTrue(h.reports.toString(), h.reports.contains("usb_connected"))
        }
    }

    @Test fun permissionResultsRecheckSystemPermissionAndDoNotRepeatTheDialog() {
        Harness().use { h ->
            h.start()
            val car = accessory()
            h.usb.setAttachedUsbAccessory(car)
            h.attached()
            assertEquals(1, h.usb.requests)
            val action = "${h.context.packageName}.CARLIFE_USB_PERMISSION"
            h.broadcast(action)
            assertTrue(h.reports.contains("usb_permission_denied"))
            assertEquals(0, h.usb.opens)
            h.attached()
            assertEquals(1, h.usb.requests)
            h.usb.grantPermission(car)
            h.broadcast(action)
            assertEquals(1, h.usb.opens)
            assertTrue(h.reports.toString(), h.reports.contains("usb_connected"))
        }
    }

    @Test fun discoveryAndOpenFailureDiagnosticsAreBoundedAndContainNoAccessoryStrings() {
        Harness().use { h ->
            h.usb.setAttachedUsbAccessory(accessory("private manufacturer", "private model"))
            h.start()
            repeat(3) { h.attached() }
            assertEquals(listOf("usb_scan state=descriptor_mismatch count=1"), h.reports.toList())
            assertEquals(0, h.usb.requests)

            val car = accessory()
            h.usb.setAttachedUsbAccessory(car)
            h.usb.grantPermission(car)
            h.usb.unavailable = true
            repeat(3) { h.attached() }
            assertEquals(1, h.reports.count { it == "usb_unavailable" })
            assertFalse(h.reports.any { it.contains("private") })
            h.usb.unavailable = false
            h.attached()
            assertTrue(h.reports.toString(), h.reports.contains("usb_connected"))
        }
    }

    @Test fun lateAttachAfterOutputCloseDoesNotScanOrRequestPermission() {
        Harness().use { h ->
            h.start()
            val scans = h.usb.scans
            h.adapter.close()
            h.usb.setAttachedUsbAccessory(accessory())
            h.activity.newIntent(Intent(UsbManager.ACTION_USB_ACCESSORY_ATTACHED))
            assertEquals(scans, h.usb.scans)
            assertEquals(0, h.usb.requests)
            assertEquals(0, h.usb.opens)
        }
    }
}
