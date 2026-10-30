// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-25
package org.openxplay.direlay.ui

import org.junit.Assert.*
import org.junit.Test
import org.openxplay.direlay.R
import org.openxplay.direlay.path.*
import org.openxplay.direlay.ui.design.stageTapSettled

class StagePresentationTest {
    private val since = 1_798_039_918_000L
    private fun step(state: StepState = StepState.ACTION, id: StepId = StepId.HOTSPOT,
        action: StepAction? = null) = PathStep(id, state, R.string.design_step_hotspot,
        detail = StepText.raw("Turn on OpenXPlay"), action = action, actionLabel = R.string.design_open)

    private fun path(focus: PathStep? = null, running: Boolean = false, canStart: Boolean = false,
        steps: List<PathStep> = listOfNotNull(focus), live: LiveSummary? = null) = ConnectionPath(
        sections = PathSection.entries.mapNotNull { section ->
            steps.filter { it.id.section == section }.takeIf { it.isNotEmpty() }?.let { PathSectionView(section, it) }
        }, focus = focus, running = running, canStart = canStart, live = live)

    @Test fun headlineTableUsesLiveThenFocusThenReadyThenIdle() {
        val focus = step().copy(evidence = StepText.raw("evidence"))
        val live = presentStage(path(focus, running = true, live = LiveSummary(since, emptyList())))
        assertEquals(StepText.of(R.string.design_live), live.headline)
        assertEquals(StageSupporting.LiveTimer(since), live.supporting)
        assertNull(live.nodes.focus)
        assertEquals(StageSupporting.LiveTimer(null),
            presentStage(path(live = LiveSummary(null, emptyList()))).supporting)
        assertEquals(focus.detail, presentStage(path(focus, canStart = true)).headline)
        assertEquals(StepText.of(focus.title), presentStage(path(focus.copy(detail = null))).headline)
        assertEquals(StepText.of(R.string.design_ready), presentStage(path(canStart = true)).headline)
        assertEquals(StepText.of(R.string.design_idle), presentStage(path()).headline)
        assertEquals(StepText.of(R.string.design_idle), presentStage(path(running = true, canStart = true)).headline)
        assertNull(presentStage(path()).supporting)
        assertNull(presentStage(path(canStart = true)).supporting)
    }

    @Test fun supportingPrefersFailedExplanationThenEvidenceThenWorkingOrWaitingElapsed() {
        val failed = step(StepState.FAILED).copy(evidence = StepText.raw("OpenXPlay"),
            explanation = R.string.design_join_explanation, sinceMillis = since)
        assertEquals(StageSupporting.Explanation(R.string.design_join_explanation), presentStage(path(failed)).supporting)
        assertEquals(StageSupporting.Evidence(failed.evidence!!, false),
            presentStage(path(failed.copy(explanation = null))).supporting)
        assertEquals(StageSupporting.Explanation(R.string.design_join_explanation),
            presentStage(path(failed.copy(evidence = null))).supporting)
        assertNull(presentStage(path(failed.copy(evidence = null, explanation = null))).supporting)
        StepState.entries.forEach { state ->
            val focus = failed.copy(state = state, evidence = null)
            val expected = when (state) {
                StepState.FAILED -> StageSupporting.Explanation(R.string.design_join_explanation)
                StepState.WORKING, StepState.WAITING -> StageSupporting.Elapsed(since)
                else -> null
            }
            assertEquals(state.name, expected, presentStage(path(focus)).supporting)
            if (state != StepState.FAILED) assertNull(presentStage(path(focus.copy(sinceMillis = null))).supporting)
        }
    }

    @Test fun aTapRightAfterTheActionChangesIsIgnored() {
        assertFalse(stageTapSettled(1_000, 1_000))
        assertFalse(stageTapSettled(1_000, 1_499))
        assertTrue(stageTapSettled(1_000, 1_500))
    }

    @Test fun carScreenWaitCarriesItsDeadlineWithoutFormattingAClockInThePresenter() {
        val focus = step(StepState.WAITING, StepId.CARPLAY_START, StepAction.START_NOW).copy(
            detail = StepText.of(R.string.path_car_screen_wait), carWaitUntilMillis = since + 20_000)
        val view = presentStage(path(focus, running = true))
        assertEquals(focus.detail, view.headline)
        assertEquals(since + 20_000, view.headlineDeadlineMillis)
        assertNull(presentStage(path(focus.copy(carWaitUntilMillis = null), running = true)).headlineDeadlineMillis)
        // The headline countdown replaces the elapsed time.
        assertNull(presentStage(path(focus.copy(sinceMillis = since), running = true)).supporting)
        assertEquals(StageSupporting.Elapsed(since),
            presentStage(path(focus.copy(sinceMillis = since, carWaitUntilMillis = null), running = true)).supporting)
        assertNull(presentStage(path(focus, running = true, live = LiveSummary(since, emptyList()))).headlineDeadlineMillis)
    }

    @Test fun onlyCarPageEvidenceOffersCopyRegardlessOfTheStepAction() {
        StepId.entries.forEach { id ->
            val focus = step(id = id, action = StepAction.COPY_PAGE_ADDRESS).copy(evidence = StepText.raw("https://relay.local"))
            assertEquals(id.name, StageSupporting.Evidence(focus.evidence!!, id == StepId.CAR_PAGE), presentStage(path(focus)).supporting)
        }
        assertEquals(StageSupporting.Evidence(StepText.raw("address"), true), presentStage(path(
            step(id = StepId.CAR_PAGE).copy(evidence = StepText.raw("address")))).supporting)
    }

    @Test fun failureCarriesOnlyFailedCodeAndItsOriginalTimestamp() {
        StepState.entries.forEach { state ->
            val focus = step(state).copy(code = "first_tcp_timeout", sinceMillis = since)
            val view = presentStage(path(focus))
            assertEquals(state.name, if (state == StepState.FAILED) focus.code else null, view.failure)
            assertEquals(state.name, if (state == StepState.FAILED) since else null, view.failureSinceMillis)
        }
        assertNull(presentStage(path(step(StepState.FAILED).copy(sinceMillis = since))).failure)
        assertNull(presentStage(path(step(StepState.FAILED).copy(sinceMillis = since))).failureSinceMillis)
        val noTime = presentStage(path(step(StepState.FAILED).copy(code = "code")))
        assertEquals("code", noTime.failure)
        assertNull(noTime.failureSinceMillis)
        assertNull(presentStage(path()).failure)
    }

    @Test fun stoppedActionTableCoversEachRowAndCountdownWinsOverFixes() {
        val fix = step(action = StepAction.OPEN_HOTSPOT)
        val action = StageAction.Step(StepAction.OPEN_HOTSPOT, fix.id, fix.actionLabel)
        val countdown = presentStage(path(fix, canStart = true), StageFlags(countdown = true))
        assertEquals(StageAction.Connect, countdown.primary)
        assertEquals(StageAction.NotNow, countdown.secondary)
        assertTrue(countdown.countdown)
        listOf(StepState.ACTION, StepState.FAILED).forEach { state ->
            listOf(false, true).forEach { ready ->
                val view = presentStage(path(fix.copy(state = state), canStart = ready))
                assertEquals(action, view.primary)
                assertEquals(if (ready) StageAction.Connect else null, view.secondary)
                assertFalse(view.countdown)
            }
        }
        val ready = presentStage(path(canStart = true))
        assertEquals(StageAction.Connect, ready.primary)
        assertNull(ready.secondary)
        val checking = presentStage(path(step(StepState.WORKING, StepId.IDENTITY)))
        assertNull(checking.primary)
        assertNull(checking.secondary)
        assertFalse(checking.countdown)
    }

    @Test fun stoppedFixRequiresActionOrFailedAndReadyCanConnectThroughOtherStates() {
        StepState.entries.forEach { state ->
            val fix = step(state, action = StepAction.OPEN_HOTSPOT)
            val isFix = state in setOf(StepState.ACTION, StepState.FAILED)
            val blocked = presentStage(path(fix))
            assertEquals(state.name, if (isFix) StageAction.Step(fix.action!!, fix.id, fix.actionLabel) else null, blocked.primary)
            assertNull(blocked.secondary)
            val ready = presentStage(path(fix, canStart = true))
            assertEquals(state.name, if (isFix) StageAction.Step(fix.action!!, fix.id, fix.actionLabel) else StageAction.Connect, ready.primary)
        }
    }

    @Test fun runningActionTableCoversLiveFixAndNoFix() {
        val fix = step(StepState.WAITING, StepId.CARPLAY_START, StepAction.START_NOW)
        val running = presentStage(path(fix, running = true))
        assertEquals(StageAction.Step(StepAction.START_NOW, fix.id, fix.actionLabel), running.primary)
        assertEquals(StageAction.Stop, running.secondary)
        val live = presentStage(path(fix, running = true, live = LiveSummary(since, emptyList())))
        assertNull(live.primary)
        assertEquals(StageAction.Stop, live.secondary)
        val waiting = presentStage(path(step(StepState.WAITING), running = true))
        assertNull(waiting.primary)
        assertEquals(StageAction.Stop, waiting.secondary)
        val empty = presentStage(path(running = true))
        assertNull(empty.primary)
        assertEquals(StageAction.Stop, empty.secondary)
    }

    @Test fun everyActionExceptCopyIsFixFirstAndKeepsStepAndOptionalLabel() {
        StepAction.entries.forEach { action ->
            val fix = step(StepState.FAILED, action = action).copy(actionLabel = null)
            val expected = if (action == StepAction.COPY_PAGE_ADDRESS) null else StageAction.Step(action, fix.id, null)
            assertEquals(action.name, expected, presentStage(path(fix)).primary)
            assertEquals(action.name, expected, presentStage(path(fix, running = true)).primary)
        }
        val copy = step(action = StepAction.COPY_PAGE_ADDRESS)
        assertEquals(StageAction.Connect, presentStage(path(copy, canStart = true)).primary)
        assertNull(presentStage(path(copy, running = true)).primary)
    }

    @Test fun countdownRequiresStoppedAndReadyAndBusyOnlyDisablesThePrimary() {
        listOf(false, true).forEach { running -> listOf(false, true).forEach { ready ->
            val input = path(canStart = ready, running = running)
            val view = presentStage(input, StageFlags(countdown = true))
            assertEquals(!running && ready, view.countdown)
            assertEquals(!running && ready, view.secondary == StageAction.NotNow)
            val busy = presentStage(input, StageFlags(countdown = true, busy = true))
            assertFalse(busy.primaryEnabled)
            assertEquals(view.copy(primaryEnabled = false), busy)
            assertTrue(view.primaryEnabled)
        } }
        val input = path(step(action = StepAction.OPEN_HOTSPOT), running = true)
        assertEquals(presentStage(input).copy(primaryEnabled = false), presentStage(input, StageFlags(busy = true)))
    }

    @Test fun noticeOrderAndVisibilityCoverAllFlagCombinations() {
        listOf(false, true).forEach { running -> listOf(false, true).forEach { display ->
            listOf(false, true).forEach { car -> listOf(false, true).forEach { setup ->
                val flags = StageFlags(displayChanged = display, carIncomplete = car, setupCompleted = setup)
                val expected = when {
                    running && display -> StageNotice.DISPLAY_CHANGED
                    car -> StageNotice.CHOOSE_CAR
                    !setup && !running -> StageNotice.SET_UP
                    else -> null
                }
                assertEquals("$running $flags", expected, presentStage(path(running = running), flags).notice)
            } }
        } }
    }

    @Test fun nodeStatesUseTheirSectionsAndCarCableIncludesPicture() {
        StepState.entries.forEach { car -> StepState.entries.forEach { picture ->
            val steps = listOf(step(StepState.OK, StepId.MODE), step(StepState.FAILED, StepId.IDENTITY),
                step(StepState.OK, StepId.IPHONE_PAIRED), step(StepState.WAITING, StepId.SESSION),
                step(StepState.OK, StepId.WEB_SERVER), step(car, StepId.CAR_PAGE), step(picture, StepId.PICTURE))
            val view = presentStage(path(steps = steps))
            val nodes = view.nodes
            assertEquals(StepState.FAILED, nodes.relay)
            assertEquals(StepState.WAITING, nodes.iphone)
            assertEquals("$car $picture", if (picture == StepState.OK) StepState.OK else car, nodes.car)
            assertEquals(LinkFill(1, 2, StepState.WAITING), nodes.iphoneLink)
            assertEquals(LinkFill(1, 2, StepState.FAILED), nodes.relayLink)
            val done = 1 + (if (car == StepState.OK) 1 else 0) + (if (picture == StepState.OK) 1 else 0)
            val cable = if (car != StepState.OK) car else if (picture == StepState.BLOCKED) StepState.OK else picture
            assertEquals("$car $picture", LinkFill(done, 3, cable), nodes.carLink)
            assertEquals(2 + done, view.done)
            assertEquals(7, view.total)
        } }
        val carOnly = presentStage(path(steps = listOf(step(StepState.ACTION, StepId.CAR_PAGE)))).nodes
        assertEquals(StepState.ACTION, carOnly.car)
        assertEquals(LinkFill(0, 1, StepState.ACTION), carOnly.carLink)
        assertEquals(LinkFill(1, 1, StepState.OK),
            presentStage(path(steps = listOf(step(StepState.OK, StepId.CAR_PAGE)))).nodes.carLink)
        val pictureOnly = presentStage(path(steps = listOf(step(StepState.OK, StepId.PICTURE)))).nodes
        assertEquals(StepState.OK, pictureOnly.car)
        assertEquals(LinkFill(1, 1, StepState.OK), pictureOnly.carLink)
        val empty = presentStage(path()).nodes
        assertEquals(StepState.BLOCKED, empty.iphone)
        assertEquals(StepState.BLOCKED, empty.relay)
        assertEquals(StepState.BLOCKED, empty.car)
        assertEquals(LinkFill(0, 0, StepState.BLOCKED), empty.iphoneLink)
        assertEquals(LinkFill(0, 0, StepState.BLOCKED), empty.carLink)
        assertEquals(LinkFill(0, 0, StepState.BLOCKED), empty.relayLink)
    }

    @Test fun relayRingUsesRelaySectionCountsAndState() {
        StepState.entries.forEach { state ->
            val view = presentStage(path(steps = listOf(step(StepState.OK, StepId.MODE), step(state, StepId.IDENTITY))))
            assertEquals(LinkFill(if (state == StepState.OK) 2 else 1, 2, state), view.nodes.relayLink)
        }
    }

    @Test fun focusMapsEverySectionAndReusesStartNowPriorityFromFocusForPage() {
        listOf(StepId.HOTSPOT to StageNode.RELAY, StepId.CAR_PAGE to StageNode.CAR,
            StepId.SESSION to StageNode.IPHONE, StepId.PICTURE to StageNode.CAR).forEach { (id, node) ->
            assertEquals(node, presentStage(path(step(id = id))).nodes.focus)
        }
        val car = step(StepState.WAITING, StepId.CAR_PAGE)
        val start = step(StepState.WAITING, StepId.CARPLAY_START, StepAction.START_NOW).copy(detail = StepText.raw("Start now"))
        val input = path(car, running = true, steps = listOf(car, start))
        val view = presentStage(input)
        assertEquals(start.detail, view.headline)
        assertEquals(StageNode.IPHONE, view.nodes.focus)
        assertEquals(StageAction.Step(StepAction.START_NOW, start.id, start.actionLabel), view.primary)
        val failed = car.copy(state = StepState.FAILED)
        assertEquals(StageNode.CAR, presentStage(input.copy(focus = failed)).nodes.focus)
        assertNull(presentStage(path()).nodes.focus)
        assertFalse(presentStage(path()).nodes.live)
        assertTrue(presentStage(path(live = LiveSummary(since, emptyList()))).nodes.live)
    }

    @Test fun handleSegmentsPreserveSectionOrderCountsAndState() {
        val sections = listOf(PathSectionView(PathSection.CAR, listOf(step(StepState.OK, StepId.WEB_SERVER), step(StepState.BLOCKED, StepId.CAR_PAGE))),
            PathSectionView(PathSection.RELAY, listOf(step(StepState.ACTION))), PathSectionView(PathSection.PICTURE, emptyList()))
        val view = presentStage(ConnectionPath(sections = sections))
        assertEquals(listOf(SectionSegment(PathSection.CAR, StepState.BLOCKED, 1, 2),
            SectionSegment(PathSection.RELAY, StepState.ACTION, 0, 1), SectionSegment(PathSection.PICTURE, StepState.OK, 0, 0)),
            view.segments)
        assertEquals(1, view.done)
        assertEquals(3, view.total)
        val empty = presentStage(path())
        assertTrue(empty.segments.isEmpty())
        assertEquals(0, empty.done)
        assertEquals(0, empty.total)
    }

    @Test fun overlayUsesExactlyTheSamePresentationAndActionTable() {
        val inputs = listOf(path(), path(canStart = true), path(step(action = StepAction.OPEN_HOTSPOT), canStart = true),
            path(step(StepState.FAILED, action = StepAction.RETRY), running = true), path(running = true, live = LiveSummary(since, emptyList())))
        inputs.forEach { input ->
            val flags = StageFlags(countdown = true, busy = true, displayChanged = true, carIncomplete = true, setupCompleted = false)
            assertEquals(presentStage(input, flags), presentStage(input, flags.copy(overlay = true)))
        }
    }
}
