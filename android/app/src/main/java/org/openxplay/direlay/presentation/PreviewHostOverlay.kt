// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-11-02
package org.openxplay.direlay.presentation

import android.content.res.ColorStateList
import android.graphics.Color
import android.graphics.drawable.GradientDrawable
import android.graphics.drawable.RippleDrawable
import android.widget.FrameLayout
import android.widget.ImageButton
import android.widget.ImageView
import androidx.activity.ComponentActivity
import androidx.core.graphics.Insets
import androidx.core.view.ViewCompat
import androidx.core.view.WindowInsetsCompat
import kotlin.math.roundToInt
import org.openxplay.direlay.R
import org.openxplay.direlay.ui.PreviewInsets
import org.openxplay.direlay.ui.previewBackPlacement

internal class PreviewHostOverlay(activity: ComponentActivity, private val canvas: () -> Pair<Int, Int>?) : FrameLayout(activity) {
    private var bars = Insets.NONE
    private val location = IntArray(2)
    private val scrim = GradientDrawable().apply { shape = GradientDrawable.OVAL }
    private val back = ImageButton(activity).apply {
        setImageResource(R.drawable.symbol_back)
        imageTintList = ColorStateList.valueOf(Color.WHITE)
        scaleType = ImageView.ScaleType.CENTER_INSIDE
        val padding = (12 * resources.displayMetrics.density).roundToInt()
        setPadding(padding, padding, padding, padding)
        contentDescription = activity.getString(R.string.host_back)
        background = RippleDrawable(ColorStateList.valueOf(0x33FFFFFF), scrim,
            GradientDrawable().apply { shape = GradientDrawable.OVAL; setColor(Color.WHITE) })
        setOnClickListener { activity.onBackPressedDispatcher.onBackPressed() }
    }

    init {
        // A native, non-clickable parent rejects touches outside the button so the CarPlay sibling receives them.
        isClickable = false
        isFocusable = false
        val size = (48 * resources.displayMetrics.density).roundToInt()
        addView(back, LayoutParams(size, size))
        ViewCompat.setOnApplyWindowInsetsListener(this) { _, insets ->
            bars = insets.getInsets(WindowInsetsCompat.Type.systemBars() or WindowInsetsCompat.Type.displayCutout())
            requestLayout()
            insets
        }
    }

    override fun onAttachedToWindow() {
        super.onAttachedToWindow()
        ViewCompat.requestApplyInsets(this)
    }

    override fun onLayout(changed: Boolean, left: Int, top: Int, right: Int, bottom: Int) {
        super.onLayout(changed, left, top, right, bottom)
        val density = resources.displayMetrics.density
        getLocationInWindow(location)
        // The host may already be inset; only the bars overlapping this view need space.
        val insets = PreviewInsets((bars.left - location[0]).coerceAtLeast(0) / density,
            (bars.top - location[1]).coerceAtLeast(0) / density,
            (location[0] + width - rootView.width + bars.right).coerceAtLeast(0) / density,
            (location[1] + height - rootView.height + bars.bottom).coerceAtLeast(0) / density)
        val picture = canvas()
        val placement = previewBackPlacement(width / density, height / density,
            picture?.first ?: 0, picture?.second ?: 0, insets, layoutDirection == LAYOUT_DIRECTION_RTL)
        val x = (placement.left * density).roundToInt()
        val y = (placement.top * density).roundToInt()
        back.layout(x, y, x + back.measuredWidth, y + back.measuredHeight)
        scrim.setColor(if (placement.scrim) 0x99000000.toInt() else Color.TRANSPARENT)
    }
}
