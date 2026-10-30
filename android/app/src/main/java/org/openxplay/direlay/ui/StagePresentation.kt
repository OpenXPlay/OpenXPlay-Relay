// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-25
package org.openxplay.direlay.ui

import androidx.compose.runtime.Immutable
import org.openxplay.direlay.R
import org.openxplay.direlay.path.*

@Immutable data class StageFlags(
    val countdown: Boolean = false,
    val busy: Boolean = false,
    val displayChanged: Boolean = false,
    val carIncomplete: Boolean = false,
    val setupCompleted: Boolean = true,
    val overlay: Boolean = false,
)

@Immutable data class StageView(
    val nodes: StageNodes,
    val headline: StepText,
    val supporting: StageSupporting?,
    val failure: String?,
    val primary: StageAction?,
    val secondary: StageAction?,
    val notice: StageNotice?,
    val segments: List<SectionSegment>,
    val done: Int,
    val total: Int,
    val failureSinceMillis: Long? = null,
    val primaryEnabled: Boolean = true,
    val countdown: Boolean = false,
    val headlineDeadlineMillis: Long? = null,
)

@Immutable data class StageNodes(
    val iphone: StepState,
    val relay: StepState,
    val car: StepState,
    val iphoneLink: LinkFill,
    val carLink: LinkFill,
    val relayLink: LinkFill,
    val focus: StageNode?,
    val live: Boolean,
)

@Immutable data class LinkFill(val done: Int, val total: Int, val state: StepState)
@Immutable enum class StageNode { IPHONE, RELAY, CAR }
@Immutable data class SectionSegment(val section: PathSection, val state: StepState, val done: Int, val total: Int)

@Immutable sealed interface StageSupporting {
    @Immutable data class Evidence(val text: StepText, val copy: Boolean) : StageSupporting
    @Immutable data class Explanation(val res: Int) : StageSupporting
    @Immutable data class Elapsed(val sinceMillis: Long) : StageSupporting
    @Immutable data class LiveTimer(val sinceMillis: Long?) : StageSupporting
}

@Immutable sealed interface StageAction {
    @Immutable data object Connect : StageAction
    @Immutable data object Stop : StageAction
    @Immutable data object NotNow : StageAction
    @Immutable data class Step(val action: StepAction, val step: StepId, val label: Int?) : StageAction
}

@Immutable enum class StageNotice { DISPLAY_CHANGED, CHOOSE_CAR, SET_UP }

fun presentStage(path: ConnectionPath, flags: StageFlags = StageFlags()): StageView {
    val focus = focusForPage(path)
    val live = path.live
    val iphone = path.section(PathSection.IPHONE)
    val relay = path.section(PathSection.RELAY)
    val car = path.section(PathSection.CAR)
    val picture = path.section(PathSection.PICTURE)
    fun PathSectionView?.link() = LinkFill(this?.done ?: 0, this?.total ?: 0, this?.state ?: StepState.BLOCKED)
    val nodes = StageNodes(
        iphone = iphone?.state ?: StepState.BLOCKED,
        relay = relay?.state ?: StepState.BLOCKED,
        car = if (picture?.state == StepState.OK) StepState.OK else car?.state ?: StepState.BLOCKED,
        iphoneLink = iphone.link(),
        // A picture that only waits for the iPhone does not dim the car steps already done.
        carLink = LinkFill((car?.done ?: 0) + (picture?.done ?: 0), (car?.total ?: 0) + (picture?.total ?: 0), when {
            car != null && car.state != StepState.OK -> car.state
            picture == null || car != null && picture.state == StepState.BLOCKED -> car?.state ?: StepState.BLOCKED
            else -> picture.state
        }),
        relayLink = relay.link(),
        focus = if (live != null) null else focus?.id?.section?.let { when (it) {
            PathSection.IPHONE -> StageNode.IPHONE
            PathSection.RELAY -> StageNode.RELAY
            PathSection.CAR, PathSection.PICTURE -> StageNode.CAR
        } },
        live = live != null,
    )
    val headline = when {
        live != null -> StepText.of(R.string.design_live)
        focus != null -> focus.detail ?: StepText.of(focus.title)
        !path.running && path.canStart -> StepText.of(R.string.design_ready)
        else -> StepText.of(R.string.design_idle)
    }
    val supporting = when {
        live != null -> StageSupporting.LiveTimer(live.sinceMillis)
        // A failure says what to do before showing the value it checked.
        focus?.state == StepState.FAILED && focus.explanation != null -> StageSupporting.Explanation(focus.explanation)
        focus?.evidence != null -> StageSupporting.Evidence(focus.evidence, focus.id == StepId.CAR_PAGE)
        // A countdown in the headline already shows time passing.
        focus?.state in setOf(StepState.WORKING, StepState.WAITING) && focus?.sinceMillis != null &&
            focus.carWaitUntilMillis == null -> StageSupporting.Elapsed(focus.sinceMillis)
        else -> null
    }
    val fix = focus?.action?.takeUnless { it == StepAction.COPY_PAGE_ADDRESS }?.let {
        StageAction.Step(it, focus.id, focus.actionLabel)
    }
    val countdown = !path.running && flags.countdown && path.canStart
    val (primary, secondary) = when {
        countdown -> StageAction.Connect to StageAction.NotNow
        !path.running && focus?.state in setOf(StepState.ACTION, StepState.FAILED) && fix != null ->
            fix to StageAction.Connect.takeIf { path.canStart }
        !path.running && path.canStart -> StageAction.Connect to null
        !path.running -> null to null
        live != null -> null to StageAction.Stop
        fix != null -> fix to StageAction.Stop
        else -> null to StageAction.Stop
    }
    val notice = when {
        path.running && flags.displayChanged -> StageNotice.DISPLAY_CHANGED
        flags.carIncomplete -> StageNotice.CHOOSE_CAR
        !flags.setupCompleted && !path.running -> StageNotice.SET_UP
        else -> null
    }
    val failure = focus?.takeIf { it.state == StepState.FAILED && it.code != null }
    return StageView(nodes, headline, supporting, failure?.code, primary, secondary, notice,
        path.sections.map { SectionSegment(it.section, it.state, it.done, it.total) },
        done = path.sections.sumOf { it.done }, total = path.sections.sumOf { it.total },
        failureSinceMillis = failure?.sinceMillis, primaryEnabled = !flags.busy, countdown = countdown,
        headlineDeadlineMillis = focus?.carWaitUntilMillis.takeIf { live == null })
}
