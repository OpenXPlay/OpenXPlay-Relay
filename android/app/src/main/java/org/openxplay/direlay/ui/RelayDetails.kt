// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-21
package org.openxplay.direlay.ui

import androidx.compose.animation.animateContentSize
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.tween
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.foundation.shape.RoundedCornerShape
import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
import android.content.pm.ApplicationInfo
import android.os.Build
import kotlinx.coroutines.delay
import java.text.DateFormat
import java.util.Date
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.rotate
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalContext
import org.openxplay.direlay.network.NetworkRequirement
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import org.openxplay.direlay.R

@OptIn(ExperimentalLayoutApi::class)
@Composable internal fun HomeProblemRow(problem: HomeProblem) {
    var expanded by rememberSaveable(problem.title, problem.text) { mutableStateOf(false) }
    val ms = if (LocalRelayMotion.current) 220 else 0
    val state = stringResource(if (expanded) R.string.menu_expanded else R.string.menu_collapsed)
    val rotation by animateFloatAsState(if (expanded) 90f else 0f, tween(ms), label = "problem expansion")
    val context = LocalContext.current
    val title = stringResource(problem.title)
    val explanation = problem.textResource?.let { stringResource(it, *problem.arguments.toTypedArray()) }
    val text = problem.text ?: if (explanation == null) stringResource(R.string.ui_error) else ""
    val time = problem.atMillis?.let {
        stringResource(R.string.error_time, DateFormat.getDateTimeInstance(DateFormat.MEDIUM, DateFormat.MEDIUM).format(Date(it)))
    }
    val detailsLabel = stringResource(R.string.error_details)
    val copied = stringResource(R.string.error_copied)
    // Some vendor builds show no clipboard confirmation, so the button itself confirms the copy.
    var justCopied by remember { mutableStateOf(false) }
    LaunchedEffect(justCopied) { if (justCopied) { delay(2000); justCopied = false } }
    // The settings action stays visible when collapsed, as before details existed.
    val settingsAction: @Composable () -> Unit = {
        problem.action?.let { action ->
            TextButton(onClick = { NetworkSettingsLauncher(context).open(action) },
                modifier = Modifier.heightIn(min = 48.dp)) { Text(stringResource(action.settingsLabel())) }
        }
    }
    Column(Modifier.fillMaxWidth().animateContentSize(tween(ms)).padding(vertical = 12.dp),
        verticalArrangement = Arrangement.spacedBy(4.dp)) {
        Row(Modifier.fillMaxWidth().heightIn(min = 48.dp)
            .semantics { stateDescription = state }.clickable(role = Role.Button) { expanded = !expanded },
            verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            Text(title, Modifier.weight(1f), style = MaterialTheme.typography.titleSmall)
            Symbol(R.drawable.symbol_chevron, state, Modifier.rotate(rotation))
        }
        if (expanded) {
            SelectionContainer {
                Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                    explanation?.let { Text(it, style = MaterialTheme.typography.bodyMedium) }
                    if (text.isNotBlank()) Text(text, style = MaterialTheme.typography.bodySmall, fontFamily = FontFamily.Monospace)
                    time?.let { Text(it, style = MaterialTheme.typography.labelSmall) }
                    problem.detail?.takeIf { it.isNotBlank() }?.let { detail ->
                        Text(detailsLabel, style = MaterialTheme.typography.labelMedium)
                        Surface(shape = RoundedCornerShape(8.dp), color = MaterialTheme.colorScheme.surfaceVariant) {
                            Text(detail, Modifier.fillMaxWidth().padding(12.dp), style = MaterialTheme.typography.bodySmall,
                                fontFamily = FontFamily.Monospace)
                        }
                    }
                }
            }
            FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                settingsAction()
                TextButton(onClick = {
                    val info = context.packageManager.getPackageInfo(context.packageName, 0)
                    val versionCode = if (Build.VERSION.SDK_INT >= 28) info.longVersionCode else info.versionCode.toLong()
                    val buildType = if (context.applicationInfo.flags and ApplicationInfo.FLAG_DEBUGGABLE != 0) "debug" else "release"
                    val environment = "${info.versionName} ($versionCode) $buildType\nAndroid ${Build.VERSION.RELEASE} (API ${Build.VERSION.SDK_INT})\n${Build.MANUFACTURER} ${Build.MODEL}"
                    val copy = listOfNotNull(title, explanation, text.takeIf { it.isNotBlank() }, time,
                        problem.detail?.takeIf { it.isNotBlank() }?.let { "$detailsLabel\n$it" }, environment).joinToString("\n\n")
                    context.getSystemService(ClipboardManager::class.java).setPrimaryClip(ClipData.newPlainText(title, copy))
                    justCopied = true
                }, modifier = Modifier.heightIn(min = 48.dp)) { Text(if (justCopied) copied else stringResource(R.string.error_copy)) }
                TextButton(onClick = { expanded = false }, modifier = Modifier.heightIn(min = 48.dp)) {
                    Text(stringResource(R.string.error_collapse))
                }
            }
        } else {
            Text((explanation ?: text).replace('\n', ' '), style = MaterialTheme.typography.bodyMedium,
                maxLines = 2, overflow = TextOverflow.Ellipsis)
            settingsAction()
        }
    }
}

@Composable internal fun FittingAddress(value: String, modifier: Modifier = Modifier) {
    BoxWithConstraints(modifier) {
        val density = LocalDensity.current
        var size by remember(value, maxWidth, density.fontScale) { mutableFloatStateOf(32f) }
        Text(value, fontSize = size.sp, maxLines = 1, softWrap = false, fontFamily = FontFamily.Monospace,
            fontWeight = FontWeight.SemiBold, letterSpacing = 0.sp,
            onTextLayout = {
                if (it.hasVisualOverflow && size > 6f) {
                    val measured = (it.getLineRight(0) - it.getLineLeft(0)).coerceAtLeast(1f)
                    size = (size * (it.size.width / measured) * .98f).coerceIn(6f, (size - .5f).coerceAtLeast(6f))
                }
            })
    }
}

@Composable internal fun ReceiverAddresses(addresses: List<String>, onCopy: (String) -> Unit, teslaUrl: String? = null) {
    addresses.forEach { address ->
        // Keep the scheme visible so typing the address does not try HTTP first.
        Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
            FittingAddress(if (address == teslaUrl) stringResource(R.string.tesla_url, address.removeSuffix("/"))
                else address.removeSuffix("/"), Modifier.weight(1f))
            IconButton(onClick = { onCopy(address) }, modifier = Modifier.sizeIn(minWidth = 48.dp, minHeight = 48.dp)) {
                Symbol(R.drawable.symbol_copy, stringResource(R.string.ui_copy))
            }
        }
    }
}

@OptIn(ExperimentalLayoutApi::class)
@Composable internal fun DetailValue(label: Int, value: String, monospace: Boolean = false) {
    FlowRow(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(12.dp),
        verticalArrangement = Arrangement.spacedBy(4.dp)) {
        Text(stringResource(label), color = MaterialTheme.colorScheme.onSurfaceVariant,
            style = MaterialTheme.typography.bodyMedium)
        Text(value, style = MaterialTheme.typography.bodyMedium,
            fontFamily = if (monospace) FontFamily.Monospace else FontFamily.Default)
    }
}

@Composable internal fun VideoStatisticsCard(value: VideoStatsPresentation) {
    CollapsibleCard("host-video-statistics", stringResource(R.string.video_stats_title),
        stringResource(value.caption.resource, *value.caption.arguments.toTypedArray())) {
        Hint(stringResource(value.caption.resource, *value.caption.arguments.toTypedArray()))
        value.sections.forEach { section ->
            Text(stringResource(section.title.resource, *section.title.arguments.toTypedArray()),
                style = MaterialTheme.typography.titleSmall)
            section.lines.forEach { Text(stringResource(it.resource, *it.arguments.toTypedArray())) }
        }
    }
}
