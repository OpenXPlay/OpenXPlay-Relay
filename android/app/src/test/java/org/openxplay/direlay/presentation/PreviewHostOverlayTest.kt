// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-11-02
package org.openxplay.direlay.presentation

import android.app.Application
import android.os.Looper
import android.view.MotionEvent
import android.view.View
import android.widget.FrameLayout
import android.widget.ImageButton
import androidx.activity.ComponentActivity
import androidx.activity.OnBackPressedCallback
import androidx.core.graphics.Insets
import androidx.core.view.ViewCompat
import androidx.core.view.WindowInsetsCompat
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import org.openxplay.direlay.R
import org.robolectric.Robolectric
import org.robolectric.RobolectricTestRunner
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [33], qualifiers = "mdpi", application = Application::class)
class PreviewHostOverlayTest {
    private fun layout(view: View, width: Int, height: Int) {
        view.measure(View.MeasureSpec.makeMeasureSpec(width, View.MeasureSpec.EXACTLY),
            View.MeasureSpec.makeMeasureSpec(height, View.MeasureSpec.EXACTLY))
        view.layout(0, 0, width, height)
    }

    @Test fun onlyButtonTouchesAreConsumedAndBackUsesTheHostDispatcher() {
        val controller = Robolectric.buildActivity(ComponentActivity::class.java).setup()
        try {
            val activity = controller.get()
            var backs = 0
            activity.onBackPressedDispatcher.addCallback(object : OnBackPressedCallback(true) {
                override fun handleOnBackPressed() { backs++ }
            })
            val forwarded = mutableListOf<Int>()
            val root = FrameLayout(activity)
            root.addView(View(activity).apply {
                setOnTouchListener { _, event -> forwarded += event.actionMasked; true }
            }, FrameLayout.LayoutParams(-1, -1))
            val overlay = PreviewHostOverlay(activity) { 2400 to 1000 }
            root.addView(overlay, FrameLayout.LayoutParams(-1, -1))
            activity.setContentView(root)
            layout(root, 900, 400)
            val back = overlay.getChildAt(0) as ImageButton
            assertFalse(overlay.isClickable)
            assertFalse(overlay.isFocusable)
            assertEquals(1, overlay.childCount)
            assertEquals(48, back.width); assertEquals(48, back.height)
            assertEquals(activity.getString(R.string.host_back), back.contentDescription)

            fun touch(action: Int, x: Float, y: Float) {
                val event = MotionEvent.obtain(0, 1, action, x, y, 0)
                try { assertTrue(root.dispatchTouchEvent(event)) } finally { event.recycle() }
            }
            touch(MotionEvent.ACTION_DOWN, 450f, 200f)
            touch(MotionEvent.ACTION_MOVE, back.x + 24, back.y + 24)
            touch(MotionEvent.ACTION_UP, back.x + 24, back.y + 24)
            assertEquals(listOf(MotionEvent.ACTION_DOWN, MotionEvent.ACTION_MOVE, MotionEvent.ACTION_UP), forwarded)
            assertEquals(0, backs)
            forwarded.clear()
            touch(MotionEvent.ACTION_DOWN, back.x + 24, back.y + 24)
            touch(MotionEvent.ACTION_UP, back.x + 24, back.y + 24)
            shadowOf(Looper.getMainLooper()).idle()
            assertTrue(forwarded.isEmpty())
            assertEquals(1, backs)
            touch(MotionEvent.ACTION_DOWN, back.x + 24, back.y + 24)
            touch(MotionEvent.ACTION_MOVE, 450f, 200f)
            touch(MotionEvent.ACTION_UP, 450f, 200f)
            shadowOf(Looper.getMainLooper()).idle()
            assertTrue(forwarded.isEmpty())
            assertEquals(1, backs)
        } finally { controller.pause().stop().destroy() }
    }

    @Test fun sizeAndInsetsChangesRepositionWithoutConsumingInsets() {
        val activity = Robolectric.buildActivity(ComponentActivity::class.java).get()
        val overlay = PreviewHostOverlay(activity) { 2400 to 1000 }
        layout(overlay, 400, 900)
        val back = overlay.getChildAt(0)
        assertEquals(4, back.left); assertEquals(159, back.top)
        layout(overlay, 900, 400)
        assertEquals(4, back.left); assertEquals(17, back.top)
        val insets = WindowInsetsCompat.Builder()
            .setInsets(WindowInsetsCompat.Type.systemBars(), Insets.of(0, 24, 0, 16))
            .setInsets(WindowInsetsCompat.Type.displayCutout(), Insets.of(30, 0, 0, 0)).build()
        val result = ViewCompat.dispatchApplyWindowInsets(overlay, insets)
        assertEquals(insets, result)
        layout(overlay, 900, 400)
        assertEquals(34, back.left); assertEquals(28, back.top)
        overlay.layoutDirection = View.LAYOUT_DIRECTION_RTL
        layout(overlay, 900, 400)
        assertEquals(848, back.left); assertEquals(28, back.top)
    }
}
