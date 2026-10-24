// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-24
package org.openxplay.direlay.ui.design

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.*
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.delay
import org.openxplay.direlay.R
import org.openxplay.direlay.path.*

@Composable fun stepText(text: StepText): String = text.raw ?: text.res?.let { stringResource(it, *text.args.toTypedArray()) }.orEmpty()

internal fun elapsedSeconds(sinceMillis: Long?, nowMillis: Long): Long =
    if (sinceMillis == null || sinceMillis >= nowMillis) 0 else (nowMillis - sinceMillis) / 1000

internal fun sessionTime(seconds: Long): String = "%02d:%02d".format(java.util.Locale.ROOT, seconds.coerceAtLeast(0) / 60, seconds.coerceAtLeast(0) % 60)

internal fun evidenceUsesMonospace(text: String): Boolean = text.none {
    it in '\u2E80'..'\u9FFF' || it in '\u3000'..'\u303F' || it in '\uF900'..'\uFAFF' || it in '\uFF00'..'\uFFEF'
}

@Composable internal fun rememberElapsedSeconds(sinceMillis: Long?): Long {
    val seconds by produceState(elapsedSeconds(sinceMillis, System.currentTimeMillis()), sinceMillis) {
        if (sinceMillis != null) while (true) {
            value = elapsedSeconds(sinceMillis, System.currentTimeMillis())
            delay(1000)
        }
    }
    return seconds
}

@Composable internal fun ElapsedTime(sinceMillis: Long?) {
    val seconds = rememberElapsedSeconds(sinceMillis)
    if (sinceMillis != null) Text(stringResource(R.string.design_seconds, seconds), style = RelayTokens.mono,
        color = MaterialTheme.colorScheme.onSurfaceVariant, maxLines = 1)
}

@Composable fun StepRow(step: PathStep, onClick: () -> Unit, onAction: (StepAction) -> Unit, focused: Boolean = false) {
    val color = LocalStatusColors.current.color(step.state)
    val detail = step.detail?.let { stepText(it) }
    val evidence = step.evidence?.let { stepText(it) }
    val state = stringResource(stateLabel(step.state))
    BoxWithConstraints(Modifier.fillMaxWidth().alpha(if (step.state == StepState.BLOCKED) .5f else 1f)) {
        val actionWidth = maxWidth * .3f
        val evidenceWidth = maxWidth * if (step.action == null) .45f else .25f
        Row(Modifier.fillMaxWidth().heightIn(min = 52.dp).semantics(mergeDescendants = true) { stateDescription = state }
            .clickable(role = Role.Button, onClick = onClick).padding(end = 12.dp),
            verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            Box(Modifier.width(2.dp).height(36.dp).background(if (focused) color else androidx.compose.ui.graphics.Color.Transparent))
            StatusGlyph(step.state, announce = false)
            Column(Modifier.weight(1f).padding(vertical = 8.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                Text(stringResource(step.title), style = RelayTokens.headline, maxLines = 1, overflow = TextOverflow.Ellipsis)
                if (detail != null || step.state == StepState.WORKING || step.state == StepState.WAITING) Row(
                    verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(4.dp)) {
                    if (detail != null) Text(detail, Modifier.weight(1f, fill = false), style = RelayTokens.footnote,
                        color = MaterialTheme.colorScheme.onSurfaceVariant, maxLines = 1, overflow = TextOverflow.Ellipsis)
                    if (step.state == StepState.WORKING || step.state == StepState.WAITING) ElapsedTime(step.sinceMillis)
                }
            }
            if (evidence != null) Text(evidence, Modifier.widthIn(max = evidenceWidth),
                style = if (evidenceUsesMonospace(evidence)) RelayTokens.mono else RelayTokens.caption,
                maxLines = 1, overflow = TextOverflow.Ellipsis, color = MaterialTheme.colorScheme.onSurfaceVariant)
            if (step.action != null) FilledTonalButton(onClick = { onAction(step.action) },
                modifier = Modifier.widthIn(max = actionWidth).heightIn(min = 48.dp), shape = RelayTokens.pill,
                contentPadding = PaddingValues(horizontal = 12.dp, vertical = 4.dp),
                colors = ButtonDefaults.filledTonalButtonColors(containerColor = color.copy(alpha = .14f), contentColor = color)) {
                Text(stringResource(step.actionLabel ?: R.string.design_open), maxLines = 1, overflow = TextOverflow.Ellipsis, style = RelayTokens.footnote)
            }
        }
    }
}
