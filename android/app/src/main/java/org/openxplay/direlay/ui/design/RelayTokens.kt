// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-24
package org.openxplay.direlay.ui.design

import androidx.compose.animation.core.FiniteAnimationSpec
import androidx.compose.animation.core.snap
import androidx.compose.animation.core.spring
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import org.openxplay.direlay.path.*
import org.openxplay.direlay.ui.LocalRelayMotion
import org.openxplay.direlay.ui.rememberRelayMotion

@Immutable data class StatusColors(val ok: Color, val working: Color, val action: Color, val failed: Color, val blocked: Color) {
    fun color(state: StepState) = when (state) {
        StepState.OK -> ok
        StepState.WORKING, StepState.WAITING -> working
        StepState.ACTION -> action
        StepState.FAILED -> failed
        StepState.BLOCKED -> blocked
    }
    fun color(tone: PathTone) = when (tone) {
        PathTone.LIVE, PathTone.READY -> ok
        PathTone.PROGRESS -> working
        PathTone.ACTION -> action
        PathTone.ERROR -> failed
        PathTone.IDLE -> blocked
    }
    fun container(state: StepState) = color(state).copy(alpha = .14f)
}

object RelayTokens {
    val darkStatus = StatusColors(Color(0xFF54E8A0), Color(0xFF64A8FF), Color(0xFFFFB547), Color(0xFFFF6961), Color(0xFF98989F).copy(alpha = .6f))
    val lightStatus = StatusColors(Color(0xFF0E9F63), Color(0xFF0A6CFF), Color(0xFFB86E00), Color(0xFFD70015), Color(0xFF6C6C70).copy(alpha = .6f))
    val darkScheme = darkColorScheme(
        primary = darkStatus.ok, onPrimary = Color(0xFF003920), primaryContainer = Color(0xFF003920), onPrimaryContainer = darkStatus.ok,
        secondary = darkStatus.working, onSecondary = Color.Black, secondaryContainer = Color(0xFF162B45), onSecondaryContainer = darkStatus.working,
        tertiary = darkStatus.action, onTertiary = Color.Black,
        background = Color.Black, onBackground = Color.White, surface = Color(0xFF1C1C1E), onSurface = Color.White,
        surfaceVariant = Color(0xFF2C2C2E), onSurfaceVariant = Color(0xFF98989F), surfaceTint = Color.Transparent,
        surfaceDim = Color.Black, surfaceBright = Color(0xFF2C2C2E), surfaceContainerLowest = Color.Black,
        surfaceContainerLow = Color(0xFF1C1C1E), surfaceContainer = Color(0xFF1C1C1E), surfaceContainerHigh = Color(0xFF2C2C2E), surfaceContainerHighest = Color(0xFF2C2C2E),
        outline = Color(0xFF38383A), outlineVariant = Color(0xFF38383A), error = darkStatus.failed,
        onError = Color.Black, errorContainer = Color(0xFF48201E), onErrorContainer = darkStatus.failed)
    val lightScheme = lightColorScheme(
        primary = lightStatus.ok, onPrimary = Color.White, primaryContainer = Color(0xFFDDF2E7), onPrimaryContainer = Color(0xFF003920),
        secondary = lightStatus.working, onSecondary = Color.White, secondaryContainer = Color(0xFFE4EEFF), onSecondaryContainer = Color(0xFF003D99),
        tertiary = lightStatus.action, onTertiary = Color.White,
        background = Color(0xFFF2F2F7), onBackground = Color.Black, surface = Color.White, onSurface = Color.Black,
        surfaceVariant = Color.White, onSurfaceVariant = Color(0xFF6C6C70), surfaceTint = Color.Transparent,
        surfaceDim = Color(0xFFF2F2F7), surfaceBright = Color.White, surfaceContainerLowest = Color.White,
        surfaceContainerLow = Color.White, surfaceContainer = Color.White, surfaceContainerHigh = Color.White, surfaceContainerHighest = Color.White,
        outline = Color(0xFFC6C6C8), outlineVariant = Color(0xFFC6C6C8), error = lightStatus.failed,
        onError = Color.White, errorContainer = Color(0xFFFFE8EA), onErrorContainer = lightStatus.failed)
    val tileInk = Color.White
    val card = RoundedCornerShape(28.dp)
    val group = RoundedCornerShape(20.dp)
    val sheet = RoundedCornerShape(topStart = 32.dp, topEnd = 32.dp)
    val pill = RoundedCornerShape(50)
    val tile = RoundedCornerShape(30)
    val chip = RoundedCornerShape(12.dp)
    val largeTitle = TextStyle(fontFeatureSettings = "tnum", fontSize = 32.sp, lineHeight = 38.sp, fontWeight = FontWeight.Bold)
    val title = TextStyle(fontFeatureSettings = "tnum", fontSize = 22.sp, lineHeight = 28.sp, fontWeight = FontWeight.SemiBold)
    val headline = TextStyle(fontFeatureSettings = "tnum", fontSize = 17.sp, lineHeight = 24.sp, fontWeight = FontWeight.SemiBold)
    val body = TextStyle(fontFeatureSettings = "tnum", fontSize = 16.sp, lineHeight = 24.sp)
    val callout = TextStyle(fontFeatureSettings = "tnum", fontSize = 15.sp, lineHeight = 20.sp)
    val footnote = TextStyle(fontFeatureSettings = "tnum", fontSize = 13.sp, lineHeight = 18.sp)
    val caption = TextStyle(fontFeatureSettings = "tnum", fontSize = 12.sp, lineHeight = 16.sp)
    val mono = caption.copy(fontFamily = FontFamily.Monospace, fontFeatureSettings = "tnum")
    val typography = Typography(displaySmall = largeTitle, headlineLarge = largeTitle, headlineMedium = title,
        headlineSmall = title, titleLarge = title, titleMedium = headline, titleSmall = headline,
        bodyLarge = body, bodyMedium = callout, bodySmall = footnote, labelLarge = callout,
        labelMedium = footnote, labelSmall = caption)
}

val LocalStatusColors = staticCompositionLocalOf { RelayTokens.darkStatus }
val LocalRelayDark = staticCompositionLocalOf { true }
// A nullable override lets the gallery exercise both palettes without changing the system setting.
val LocalRelayDarkOverride = staticCompositionLocalOf<Boolean?> { null }

@Composable fun RelayDesignTheme(dark: Boolean = LocalRelayDarkOverride.current ?: isSystemInDarkTheme(), content: @Composable () -> Unit) {
    val motion = rememberRelayMotion()
    CompositionLocalProvider(LocalStatusColors provides if (dark) RelayTokens.darkStatus else RelayTokens.lightStatus,
        LocalRelayDark provides dark, LocalRelayMotion provides (LocalRelayMotion.current && motion)) {
        // Material 3 1.4.0 keeps its expressive theme and MotionScheme factory internal.
        MaterialTheme(colorScheme = if (dark) RelayTokens.darkScheme else RelayTokens.lightScheme,
            typography = RelayTokens.typography, shapes = Shapes(RelayTokens.chip, RelayTokens.chip, RelayTokens.group, RelayTokens.card, RelayTokens.card),
            content = content)
    }
}

internal fun <T> relaySpatial(motion: Boolean): FiniteAnimationSpec<T> = if (motion) spring(dampingRatio = .8f, stiffness = 380f) else snap()
internal fun <T> relayEffects(motion: Boolean): FiniteAnimationSpec<T> = if (motion) spring(dampingRatio = 1f, stiffness = 1600f) else snap()

enum class IconHue { BLUETOOTH, NETWORK, CAR, IPHONE, IDENTITY, PERMISSIONS, PICTURE, LOGS, DEVELOPER }

@Composable fun iconHue(hue: IconHue): Color = when (hue) {
    IconHue.BLUETOOTH -> LocalStatusColors.current.working
    IconHue.NETWORK -> if (LocalRelayDark.current) Color(0xFF30BFB8) else Color(0xFF008A83)
    IconHue.CAR -> if (LocalRelayDark.current) Color(0xFF8182EF) else Color(0xFF5856D6)
    IconHue.IPHONE -> if (LocalRelayDark.current) Color(0xFF636366) else Color(0xFF48484A)
    IconHue.IDENTITY -> if (LocalRelayDark.current) Color(0xFFBF7AF0) else Color(0xFF8944AB)
    IconHue.PERMISSIONS -> LocalStatusColors.current.action
    IconHue.PICTURE -> if (LocalRelayDark.current) Color(0xFFED7AAA) else Color(0xFFD93675)
    IconHue.LOGS -> if (LocalRelayDark.current) Color(0xFF8E8E93) else Color(0xFF636366)
    IconHue.DEVELOPER -> if (LocalRelayDark.current) Color(0xFF75899E) else Color(0xFF52697E)
}
