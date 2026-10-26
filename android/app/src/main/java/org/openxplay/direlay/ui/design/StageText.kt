// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-25
package org.openxplay.direlay.ui.design

import android.view.HapticFeedbackConstants
import androidx.compose.animation.*
import androidx.compose.animation.core.tween
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.*
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.delay
import org.openxplay.direlay.R
import org.openxplay.direlay.path.StepText
import org.openxplay.direlay.path.countdownSeconds
import org.openxplay.direlay.ui.*
import java.text.DateFormat
import java.text.SimpleDateFormat
import java.util.Date
import java.util.Locale

@Composable fun StageHeadline(view: StageView, onCopy: (String) -> Unit, modifier: Modifier = Modifier,
    textAlign: TextAlign = TextAlign.Center) {
    val motion = LocalRelayMotion.current
    val transform = stageTextTransform(motion)
    val arrival = stageArrival()
    val alignment = if (textAlign == TextAlign.Start) Alignment.Start else Alignment.CenterHorizontally
    Column(modifier.fillMaxWidth().graphicsLayer {
        alpha = arrival.value
        translationY = 8.dp.toPx() * (1f - arrival.value)
    }, horizontalAlignment = alignment, verticalArrangement = Arrangement.spacedBy(8.dp)) {
        AnimatedContent(view.headline to view.headlineDeadlineMillis, transitionSpec = { transform }, label = "stage headline") { (headline, deadline) ->
            StageHeadlineText(headline, deadline, textAlign)
        }
        AnimatedContent(view.supporting, transitionSpec = { transform }, label = "stage supporting") { supporting ->
            if (supporting != null) StageSupportingText(supporting, onCopy, textAlign = textAlign)
        }
        AnimatedContent(view.failure to view.failureSinceMillis, transitionSpec = { transform }, label = "stage failure") { (code, since) ->
            if (code != null) StageFailureLine(code, since, textAlign = textAlign)
        }
    }
}

@Composable private fun StageHeadlineText(headline: StepText, deadline: Long?, textAlign: TextAlign) {
    val text = if (deadline == null) stepText(headline) else {
        val seconds by produceState(countdownSeconds(deadline, System.currentTimeMillis()), deadline) {
            while (value > 0) {
                delay(1000)
                value = countdownSeconds(deadline, System.currentTimeMillis())
            }
        }
        headline.res?.let { stringResource(it, seconds.toString()) } ?: stepText(headline)
    }
    Text(text, Modifier.fillMaxWidth().semantics { liveRegion = LiveRegionMode.Polite },
        style = RelayTokens.title, textAlign = textAlign)
}

@Composable fun StageSupportingText(supporting: StageSupporting, onCopy: (String) -> Unit,
    modifier: Modifier = Modifier, textAlign: TextAlign = TextAlign.Center) {
    when (supporting) {
        is StageSupporting.Evidence -> StageEvidence(stepText(supporting.text), supporting.copy, onCopy, modifier, textAlign)
        is StageSupporting.Explanation -> SupportingText(stringResource(supporting.res), modifier, textAlign)
        is StageSupporting.Elapsed -> StageElapsed(supporting.sinceMillis, modifier, textAlign)
        is StageSupporting.LiveTimer -> StageLiveTimer(supporting.sinceMillis, modifier, textAlign)
    }
}

@Composable private fun SupportingText(text: String, modifier: Modifier, textAlign: TextAlign, mono: Boolean = false) {
    Text(text, modifier.fillMaxWidth(), style = if (mono) RelayTokens.mono else RelayTokens.callout,
        color = MaterialTheme.colorScheme.onSurfaceVariant, textAlign = textAlign)
}

@Composable private fun StageElapsed(sinceMillis: Long, modifier: Modifier, textAlign: TextAlign) {
    val seconds = rememberElapsedSeconds(sinceMillis)
    SupportingText(stringResource(R.string.design_seconds, seconds), modifier, textAlign, mono = true)
}

@Composable private fun StageLiveTimer(sinceMillis: Long?, modifier: Modifier, textAlign: TextAlign) {
    val seconds = rememberElapsedSeconds(sinceMillis)
    SupportingText(sessionTime(seconds), modifier, textAlign, mono = true)
}

@Composable fun StageFailureLine(code: String, sinceMillis: Long?, modifier: Modifier = Modifier,
    textAlign: TextAlign = TextAlign.Center) {
    val configuration = LocalConfiguration.current
    val text = remember(code, sinceMillis, configuration) {
        val format: DateFormat = SimpleDateFormat("HH:mm:ss", Locale.getDefault())
        if (sinceMillis == null) code else "$code · ${format.format(Date(sinceMillis))}"
    }
    Text(text, modifier.fillMaxWidth(), style = RelayTokens.mono, color = LocalStatusColors.current.failed, textAlign = textAlign)
}

@Composable private fun StageEvidence(text: String, copy: Boolean, onCopy: (String) -> Unit,
    modifier: Modifier, textAlign: TextAlign) {
    if (!copy) {
        SupportingText(text, modifier, textAlign, mono = evidenceUsesMonospace(text))
        return
    }
    val motion = LocalRelayMotion.current
    val view = LocalView.current
    val transform = if (motion) ContentTransform(fadeIn(tween(100)) + scaleIn(tween(100), initialScale = .8f),
        fadeOut(tween(100)) + scaleOut(tween(100), targetScale = .8f), sizeTransform = SizeTransform { _, _ -> tween(100) })
        else ContentTransform(EnterTransition.None, ExitTransition.None, sizeTransform = null)
    var copies by remember(text) { mutableIntStateOf(0) }
    var copied by remember(text) { mutableStateOf(false) }
    LaunchedEffect(text, copies) {
        if (copies > 0) {
            copied = true
            delay(1200)
            copied = false
        }
    }
    val label = stringResource(if (copied) R.string.design_copied else R.string.ui_copy)
    Row(modifier.fillMaxWidth().heightIn(min = 48.dp).semantics(mergeDescendants = true) { stateDescription = label }
        .clickable(role = Role.Button, onClickLabel = stringResource(R.string.ui_copy)) {
            onCopy(text)
            view.performHapticFeedback(HapticFeedbackConstants.CLOCK_TICK)
            copies++
        }.padding(horizontal = 8.dp, vertical = 4.dp), verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(8.dp, if (textAlign == TextAlign.Start) Alignment.Start else Alignment.CenterHorizontally)) {
        Text(text, Modifier.weight(1f, fill = false), style = if (evidenceUsesMonospace(text)) RelayTokens.mono else RelayTokens.callout,
            color = MaterialTheme.colorScheme.onSurfaceVariant, textAlign = textAlign)
        AnimatedContent(copied, transitionSpec = { transform }, label = "stage copy") { checked ->
            Symbol(if (checked) R.drawable.symbol_check else R.drawable.symbol_copy, null, Modifier.size(18.dp))
        }
    }
}
