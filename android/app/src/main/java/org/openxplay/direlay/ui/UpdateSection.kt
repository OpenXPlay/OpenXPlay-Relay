// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-25
package org.openxplay.direlay.ui

import android.content.Intent
import android.net.Uri
import androidx.compose.foundation.layout.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.*
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import org.openxplay.direlay.R
import org.openxplay.direlay.ui.design.*
import org.openxplay.direlay.update.*
import java.time.ZoneId
import java.time.format.DateTimeFormatter
import java.time.format.FormatStyle

internal val LocalUpdateChecker = staticCompositionLocalOf<org.openxplay.direlay.presentation.UpdateActions?> { null }

@Composable internal fun updateMenuVersion(version: String): String {
    val checker = LocalUpdateChecker.current ?: return version
    val state by checker.state.screenState()
    val update = state.result as? UpdateResult.Available ?: return version
    return stringResource(R.string.update_menu_version, version, update.release.version.value)
}

@Composable internal fun UpdateSection(checker: org.openxplay.direlay.presentation.UpdateActions) {
    val state by checker.state.screenState()
    val context = LocalContext.current
    val locale = LocalConfiguration.current.locales[0]
    val checking = stringResource(R.string.update_checking)
    val value = when {
        state.checking -> null
        state.failure != null -> stringResource(R.string.update_failed)
        state.result is UpdateResult.Available -> (state.result as UpdateResult.Available).release.version.value
        state.result is UpdateResult.UpToDate -> stringResource(R.string.update_up_to_date, (state.result as UpdateResult.UpToDate).release.version.value)
        state.result == UpdateResult.NoRelease -> stringResource(R.string.update_no_release)
        else -> null
    }
    InsetGroup(footer = stringResource(R.string.update_channel_hint)) {
        InsetRow(R.drawable.symbol_sync, IconHue.DEVELOPER, stringResource(R.string.update_check), value,
            onClick = checker.check, trailing = {
                if (state.checking) CircularProgressIndicator(Modifier.size(20.dp).semantics { contentDescription = checking }, strokeWidth = 2.dp)
            })
        state.failure?.let { Text(updateFailureText(it), Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 8.dp),
            style = RelayTokens.footnote, color = MaterialTheme.colorScheme.error) }
        InsetDivider()
        Box(Modifier.padding(16.dp)) {
            Options(listOf(stringResource(R.string.update_stable), stringResource(R.string.update_beta)), state.channel.ordinal) {
                checker.setChannel(UpdateChannel.entries[it])
            }
        }
    }
    val release = (state.result as? UpdateResult.Available)?.release ?: return
    val notes = remember(release.body, locale.language) { ReleaseNotes.localized(release.body, locale.language == "zh") }
    InsetGroup {
        Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
            Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                Text(release.version.value, Modifier.weight(1f), style = RelayTokens.headline)
                Text(DateTimeFormatter.ISO_LOCAL_DATE.withZone(ZoneId.systemDefault()).format(release.publishedAt), Modifier.weight(1f),
                    style = RelayTokens.callout, textAlign = TextAlign.End, color = MaterialTheme.colorScheme.onSurfaceVariant)
            }
            if (notes.summary.isNotBlank()) Text(notes.summary, style = RelayTokens.body)
            Button(onClick = { context.startActivity(Intent(Intent.ACTION_VIEW, Uri.parse(release.apkUrl ?: release.htmlUrl))) },
                modifier = Modifier.fillMaxWidth().heightIn(min = 48.dp), shape = RelayTokens.chip) {
                Symbol(R.drawable.symbol_download, null, Modifier.size(20.dp))
                Spacer(Modifier.width(8.dp))
                Text(if (release.apkUrl == null) stringResource(R.string.update_open_release) else stringResource(R.string.update_download, release.version.value),
                    Modifier.weight(1f), textAlign = TextAlign.Center)
            }
        }
    }
    SettingsGroup("update-notes:${release.tag}", R.drawable.symbol_info, IconHue.DEVELOPER, stringResource(R.string.update_notes)) {
        notes.blocks.forEach { block ->
            when (block) {
                is ReleaseNoteBlock.Heading -> Text(block.text, Modifier.padding(top = 8.dp).semantics { heading() }, style = RelayTokens.headline)
                is ReleaseNoteBlock.Bullet -> Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    Text("•", style = RelayTokens.body)
                    Text(block.text, Modifier.weight(1f), style = RelayTokens.body)
                }
                is ReleaseNoteBlock.Paragraph -> Text(block.text, style = RelayTokens.body)
            }
        }
    }
}

@Composable private fun updateFailureText(failure: UpdateFailure): String = when (failure) {
    UpdateFailure.NoInternet -> stringResource(R.string.update_no_internet)
    UpdateFailure.Unreachable -> stringResource(R.string.update_unreachable)
    UpdateFailure.Timeout -> stringResource(R.string.update_timeout)
    is UpdateFailure.Http -> stringResource(R.string.update_http_error, failure.status)
    UpdateFailure.InvalidResponse -> stringResource(R.string.update_invalid_response)
    is UpdateFailure.RateLimited -> {
        val locale = LocalConfiguration.current.locales[0]
        val reset = failure.resetAt?.let { runCatching {
            DateTimeFormatter.ofLocalizedDateTime(FormatStyle.SHORT).withLocale(locale).withZone(ZoneId.systemDefault()).format(it)
        }.getOrNull() }
        if (reset == null) stringResource(R.string.update_rate_limited) else stringResource(R.string.update_rate_limited_until, reset)
    }
}
